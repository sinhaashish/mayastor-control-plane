pub(crate) mod affinity_group;
pub(crate) mod nexus;
pub(crate) mod pool;
pub(crate) mod resources;
pub(crate) mod volume;
mod volume_policy;

use crate::controller::scheduling::{
    nexus::{GetPersistedNexusChildrenCtx, GetSuitableNodesContext},
    resources::{ChildItem, NodeItem, PoolItem, ReplicaItem},
    volume::{GetSuitablePoolsContext, ReplicaResizePoolsContext, VolumeReplicasForNexusCtx},
};
use std::{cmp::Ordering, collections::HashMap, future::Future};
use stor_port::types::v0::transport::NodeTopology;
use weighted_scoring::{Criteria, Value, ValueGrading, WeightedScore};

#[async_trait::async_trait(?Send)]
pub(crate) trait ResourcePolicy<Request: ResourceFilter>: Sized {
    fn apply(self, to: Request) -> Request;
    fn apply_async(self, to: Request) -> Request {
        self.apply(to)
    }
}

/// Default container of context and a list of items which must be filtered down and sorted.
#[derive(Clone)]
pub(crate) struct ResourceData<C, I: std::fmt::Debug> {
    context: C,
    list: Vec<I>,
}
impl<C, I: std::fmt::Debug> ResourceData<C, I> {
    /// Create a new `Self`.
    pub(crate) fn new(request: C, list: Vec<I>) -> Self {
        Self {
            context: request,
            list,
        }
    }
    pub(crate) fn context(&self) -> &C {
        &self.context
    }
}

#[async_trait::async_trait(?Send)]
pub(crate) trait ResourceFilter: Sized {
    type Request;
    type Item: std::fmt::Debug;

    fn data(&mut self) -> &mut ResourceData<Self::Request, Self::Item>;

    fn policy<P: ResourcePolicy<Self>>(self, policy: P) -> Self {
        policy.apply(self)
    }
    fn policy_async<P: ResourcePolicy<Self>>(self, policy: P) -> Self {
        policy.apply_async(self)
    }
    fn filter_param<P, F>(mut self, param: &P, filter: F) -> Self
    where
        F: Fn(&P, &Self::Request, &Self::Item) -> bool,
    {
        let data = self.data();
        data.list.retain(|v| filter(param, &data.context, v));
        self
    }
    fn filter_iter(self, filter: fn(Self) -> Self) -> Self {
        filter(self)
    }
    async fn filter_iter_async<F, Fut>(self, filter: F) -> Self
    where
        F: Fn(Self) -> Fut,
        Fut: Future<Output = Self>,
    {
        filter(self).await
    }
    fn filter<F: FnMut(&Self::Request, &Self::Item) -> bool>(mut self, mut filter: F) -> Self {
        let data = self.data();
        data.list.retain(|v| filter(&data.context, v));
        self
    }
    fn sort<F: FnMut(&Self::Item, &Self::Item) -> std::cmp::Ordering>(mut self, sort: F) -> Self {
        let data = self.data();
        data.list.sort_by(sort);
        self
    }
    fn sort_ctx<F: FnMut(&Self::Request, &Self::Item, &Self::Item) -> std::cmp::Ordering>(
        mut self,
        mut sort: F,
    ) -> Self {
        let data = self.data();
        data.list.sort_by(|a, b| sort(&data.context, a, b));
        self
    }
    fn collect(self) -> Vec<Self::Item>;
    fn group_by<K, V, F: Fn(&Self::Request, &Vec<Self::Item>) -> HashMap<K, V>>(
        mut self,
        group: F,
    ) -> HashMap<K, V> {
        let data = self.data();
        group(&data.context, &data.list)
    }
}

/// Represents a sort criteria to be passed to a sort builder.
pub(crate) struct SortCriteria {
    criteria: Criteria,
    grading: ValueGrading,
    value_fn: Box<dyn Fn(&PoolItem) -> Value>,
}

impl SortCriteria {
    /// Create a new sort criteria.
    pub(crate) fn new(
        criteria: Criteria,
        grading: ValueGrading,
        value_fn: impl Fn(&PoolItem) -> Value + 'static,
    ) -> Self {
        SortCriteria {
            criteria,
            grading,
            value_fn: Box::new(value_fn),
        }
    }
}

/// Builds a weighted sorting comparator, with the various sort criterias being added to it.
pub(crate) struct SortBuilder {
    sort_criterias: Vec<SortCriteria>,
}

impl SortBuilder {
    /// Create a new sort builder.
    pub(crate) fn new() -> Self {
        SortBuilder {
            sort_criterias: Vec::new(),
        }
    }

    /// Add sort criteria to the builder.
    pub(crate) fn with_criteria(mut self, sort_criteria: fn() -> SortCriteria) -> Self {
        self.sort_criterias.push(sort_criteria());
        self
    }

    /// Build the comparator based on the weights of sort criteria.
    pub(crate) fn compare(&self, a: &PoolItem, b: &PoolItem) -> std::cmp::Ordering {
        let mut weighted_score = WeightedScore::dual_values();
        for criteria in &self.sort_criterias {
            let value_a = (criteria.value_fn)(a);
            let value_b = (criteria.value_fn)(b);
            weighted_score =
                weighted_score.weigh(criteria.criteria, criteria.grading, value_a, value_b);
        }
        let (score_a, score_b) = weighted_score.score().unwrap();
        score_b.cmp(&score_a)
    }
}

/// Filter nodes used for replica creation
pub(crate) struct NodeFilters {}
impl NodeFilters {
    /// Should only attempt to use online nodes for pools.
    pub(crate) fn online_for_pool(_request: &GetSuitablePoolsContext, item: &PoolItem) -> bool {
        item.node.is_online()
    }
    /// Should only attempt to use allowed nodes (by the topology).
    pub(crate) fn allowed(request: &GetSuitablePoolsContext, item: &PoolItem) -> bool {
        request.allowed_nodes().is_empty() || request.allowed_nodes().contains(&item.pool.node)
    }
    /// Should only attempt to use nodes not currently used by the volume.
    /// When moving a replica the current replica node is allowed to be reused for a different pool.
    pub(crate) fn unused(request: &GetSuitablePoolsContext, item: &PoolItem) -> bool {
        if let Some(moving) = request.move_repl() {
            if moving.node() == &item.pool.node && moving.pool() != &item.pool.id {
                return true;
            }
        }
        let registry = request.registry();
        let used_nodes = registry.specs().volume_data_nodes(&request.uuid);
        !used_nodes.contains(&item.pool.node)
    }
    /// Should only attempt to use nodes which are not cordoned.
    pub(crate) fn cordoned_for_pool(request: &GetSuitablePoolsContext, item: &PoolItem) -> bool {
        let registry = request.registry();
        !registry
            .specs()
            .cordoned_nodes()
            .into_iter()
            .any(|node_spec| node_spec.id() == &item.pool.node)
    }

    /// Should only attempt to use online nodes.
    pub(crate) fn online(_request: &GetSuitableNodesContext, item: &NodeItem) -> bool {
        item.node_wrapper().is_online()
    }

    /// Should only attempt to use nodes which are not cordoned.
    pub(crate) fn cordoned(request: &GetSuitableNodesContext, item: &NodeItem) -> bool {
        let registry = request.registry();
        !registry
            .specs()
            .cordoned_nodes()
            .into_iter()
            .any(|node_spec| node_spec.id() == item.node_wrapper().id())
    }

    /// Should only attempt to use node where current target is not present.
    pub(crate) fn current_target(request: &GetSuitableNodesContext, item: &NodeItem) -> bool {
        if let Some(target) = request.target() {
            target.node() != item.node_wrapper().id()
        } else {
            true
        }
    }
    /// Should only attempt to use nodes having specific creation label if topology has it.
    pub(crate) fn topology(request: &GetSuitableNodesContext, item: &NodeItem) -> bool {
        println!("ASHISH");
        let volume_node_topology_inclusion_labels: HashMap<String, String>;
        let volume_node_topology_exclusion_labels: HashMap<String, String>;
        match request.topology.clone() {
            None => return true,
            Some(topology) => match topology.node {
                None => return true,
                Some(node_topology) => match node_topology {
                    NodeTopology::Labelled(labelled_topology) => {
                        // Return false if the exclusion and incluson labels has any common key.
                        if labelled_topology
                            .inclusion
                            .keys()
                            .any(|key| labelled_topology.exclusion.contains_key(key))
                        {
                            return false;
                        }

                        if !labelled_topology.inclusion.is_empty()
                            || !labelled_topology.exclusion.is_empty()
                        {
                            volume_node_topology_inclusion_labels = labelled_topology.inclusion;
                            volume_node_topology_exclusion_labels = labelled_topology.exclusion;
                        } else {
                            return true;
                        }
                    }
                    NodeTopology::Explicit(_) => todo!(),
                },
            },
        };

        // We will reach this part of code only if the volume has node inclusion/exclusion labels.
        match request
            .registry()
            .specs()
            .node(&item.node_wrapper.node_state.id())
        {
            Ok(spec) => {
                // Inclusion condition
                let inc_qualify = does_node_qualify_inclusion_labels(
                    volume_node_topology_inclusion_labels,
                    spec.labels,
                );
                // let exc_qualify = does_node_qualify_exclusion_labels(
                //     volume_node_topology_exclusion_labels,
                //     spec.labels,
                // );
                return inc_qualify;
            }
            Err(_) => false,
        }
    }
    /// Should only attempt to use node where there are no targets for the current volume.
    pub(crate) fn no_targets(request: &GetSuitableNodesContext, item: &NodeItem) -> bool {
        let volume_targets = request.registry().specs().volume_nexuses(&request.uuid);
        !volume_targets
            .into_iter()
            .any(|n| &n.lock().node == item.node_wrapper().id())
    }
}

/// Retruns true if all the keys in volume inclusive labels
/// matches to the node labels; otherwise returns false
pub(crate) fn does_node_qualify_inclusion_labels(
    vol_inc_labels: HashMap<String, String>,
    node_labels: HashMap<String, String>,
) -> bool {
    println!("vol_inc_labels {:?}", vol_inc_labels);
    println!("node_labels {:?}", node_labels);
    let mut inc_match = true; // Initialize to true, assuming inclusive match until proven otherwise
    for (vol_inc_key, vol_inc_value) in vol_inc_labels.iter() {
        match node_labels.get(vol_inc_key) {
            Some(node_val) => {
                if node_val != vol_inc_value {
                    inc_match = false;
                    break; // No need to continue checking once a mismatch is found
                }
            }
            None => {
                inc_match = false;
                break; // No need to continue checking if a key is not present
            }
        }
    }
    inc_match
}


// /// Retruns true if all the keys in volume inclusive labels
// /// matches to the node labels; otherwise returns false
// pub(crate) fn does_node_qualify_exclusion_labels(
//     vol_exc_labels: HashMap<String, String>,
//     node_labels: HashMap<String, String>,
// ) -> bool {
//     let mut inc_match = true; // Initialize to true, assuming inclusive match until proven otherwise
//     for (vol_inc_key, vol_inc_value) in vol_exc_labels.iter() {
//         match node_labels.get(vol_inc_key) {
//             Some(node_val) => {
//                 if node_val != vol_inc_value {
//                     inc_match = false;
//                     break; // No need to continue checking once a mismatch is found
//                 }
//             }
//             None => {
//                 inc_match = false;
//                 break; // No need to continue checking if a key is not present
//             }
//         }
//     }
//     inc_match
// }

/// Sort the nexus children for removal when decreasing a volume's replica count
pub(crate) struct ChildSorters {}
impl ChildSorters {
    /// Sort replicas by their nexus child (state and rebuild progress)
    /// todo: should we use weights instead (like moac)?
    pub(crate) fn sort(a: &ReplicaItem, b: &ReplicaItem) -> std::cmp::Ordering {
        match Self::sort_by_health(a, b) {
            Ordering::Equal => match Self::sort_by_child(a, b) {
                Ordering::Equal => {
                    let childa_is_local = !a.spec().share.shared();
                    let childb_is_local = !b.spec().share.shared();
                    if childa_is_local == childb_is_local {
                        match a.ag_replicas_on_pool().cmp(&b.ag_replicas_on_pool()) {
                            Ordering::Less => Ordering::Greater,
                            Ordering::Equal => Ordering::Equal,
                            Ordering::Greater => Ordering::Less,
                        }
                    } else if childa_is_local {
                        std::cmp::Ordering::Greater
                    } else {
                        std::cmp::Ordering::Less
                    }
                }
                ord => ord,
            },
            ord => ord,
        }
    }
    // sort replicas by their health: prefer healthy replicas over unhealthy
    fn sort_by_health(a: &ReplicaItem, b: &ReplicaItem) -> std::cmp::Ordering {
        match a.child_info() {
            None => {
                match b.child_info() {
                    Some(b_info) if b_info.healthy => {
                        // sort replicas by their health: prefer healthy replicas over unhealthy
                        std::cmp::Ordering::Less
                    }
                    _ => std::cmp::Ordering::Equal,
                }
            }
            Some(a_info) => match b.child_info() {
                Some(b_info) if a_info.healthy && !b_info.healthy => std::cmp::Ordering::Greater,
                Some(b_info) if !a_info.healthy && b_info.healthy => std::cmp::Ordering::Less,
                _ => std::cmp::Ordering::Equal,
            },
        }
    }
    // remove unused replicas first
    fn sort_by_child(a: &ReplicaItem, b: &ReplicaItem) -> std::cmp::Ordering {
        match a.child_spec() {
            None => {
                match b.child_spec() {
                    None => std::cmp::Ordering::Equal,
                    Some(_) => {
                        // prefer the replica that is not part of a nexus
                        std::cmp::Ordering::Greater
                    }
                }
            }
            Some(_) => {
                match b.child_spec() {
                    // prefer the replica that is not part of a nexus
                    None => std::cmp::Ordering::Less,
                    // compare the child states, and then the rebuild progress
                    Some(_) => match (a.child_state(), b.child_state()) {
                        (Some(a_state), Some(b_state)) => {
                            match a_state.state.partial_cmp(&b_state.state) {
                                None => a_state.rebuild_progress.cmp(&b_state.rebuild_progress),
                                Some(ord) => ord,
                            }
                        }
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    },
                }
            }
        }
    }
}

/// Filter the nexus children/replica candidates when creating a nexus
pub(crate) struct ChildInfoFilters {}
impl ChildInfoFilters {
    /// Should only allow healthy children
    pub(crate) fn healthy(request: &GetPersistedNexusChildrenCtx, item: &ChildItem) -> bool {
        // on first creation there is no nexus_info/child_info so all children are deemed healthy
        let first_create = request.nexus_info().is_none();
        first_create || item.info().as_ref().map(|i| i.healthy).unwrap_or(false)
    }
}

/// Filter the nexus children/replica candidates when creating a nexus
pub(crate) struct ReplicaFilters {}
impl ReplicaFilters {
    /// Should only allow children with corresponding online replicas
    pub(crate) fn online(_request: &GetPersistedNexusChildrenCtx, item: &ChildItem) -> bool {
        item.state().online()
    }

    /// Should only try to resize online replicas
    pub(crate) fn online_for_resize(
        _request: &ReplicaResizePoolsContext,
        item: &ChildItem,
    ) -> bool {
        item.state().online()
    }

    /// Should only allow children with corresponding replicas with enough size
    pub(crate) fn size(request: &GetPersistedNexusChildrenCtx, item: &ChildItem) -> bool {
        match request.vol_spec() {
            Some(volume) => item.state().size >= volume.size,
            None => true,
        }
    }

    /// Should only allow children which are reservable.
    pub(crate) fn reservable(request: &GetPersistedNexusChildrenCtx, item: &ChildItem) -> bool {
        !request.shutdown_failed_nexuses().iter().any(|p| {
            let nexus = p.lock();
            nexus.node == item.pool().node && nexus.contains_replica(&item.spec().uuid)
        })
    }
}

/// Sort the nexus replicas/children by preference when creating a nexus
pub(crate) struct ChildItemSorters {}
impl ChildItemSorters {
    /// Sort ChildItem's for volume nexus creation
    /// Prefer children local to where the nexus will be created
    pub(crate) fn sort_by_locality(
        request: &GetPersistedNexusChildrenCtx,
        a: &ChildItem,
        b: &ChildItem,
    ) -> std::cmp::Ordering {
        let a_is_local = Some(&a.state().node) == request.target_node();
        let b_is_local = Some(&b.state().node) == request.target_node();
        match (a_is_local, b_is_local) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (_, _) => std::cmp::Ordering::Equal,
        }
    }
}

/// Filter replicas when selecting the best candidates to add to a nexus
pub(crate) struct AddReplicaFilters {}
impl AddReplicaFilters {
    /// Should only allow children with corresponding online replicas
    pub(crate) fn online(_request: &VolumeReplicasForNexusCtx, item: &ChildItem) -> bool {
        item.state().online()
    }

    /// Should only allow children with corresponding replicas with enough size
    pub(crate) fn size(request: &VolumeReplicasForNexusCtx, item: &ChildItem) -> bool {
        item.state().size >= request.vol_spec().size
    }

    /// Should only allow children which are reservable.
    pub(crate) fn reservable(request: &VolumeReplicasForNexusCtx, item: &ChildItem) -> bool {
        !request.shutdown_failed_nexuses().iter().any(|p| {
            let nexus = p.lock();
            nexus.node == item.pool().node && nexus.contains_replica(&item.spec().uuid)
        })
    }
}

/// Sort replicas to pick the best choice to add to a given nexus
pub(crate) struct AddReplicaSorters {}
impl AddReplicaSorters {
    /// Sorted by:
    /// 1. replicas local to the nexus
    /// 2. replicas which have not been marked as faulted by the io-engine
    /// 3. replicas from pools with more free space
    pub(crate) fn sort(
        request: &VolumeReplicasForNexusCtx,
        a: &ChildItem,
        b: &ChildItem,
    ) -> std::cmp::Ordering {
        let a_is_local = a.state().node == request.nexus_spec().node;
        let b_is_local = b.state().node == request.nexus_spec().node;
        match (a_is_local, b_is_local) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (_, _) => {
                let a_healthy = a.info().as_ref().map(|i| i.healthy).unwrap_or(false);
                let b_healthy = b.info().as_ref().map(|i| i.healthy).unwrap_or(false);
                match (a_healthy, b_healthy) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    (_, _) => a.pool().free_space().cmp(&b.pool().free_space()),
                }
            }
        }
    }
}

/// Sort nodes to pick the best choice for nexus target.
pub(crate) struct NodeSorters {}
impl NodeSorters {
    /// Sort nodes by the number of active nexus present per node.
    /// The lesser the number of active nexus on a node, the more would be its selection priority.
    /// In case this is a Affinity Group, then it would be spread on basis of number of ag targets
    /// and then on basis of total targets on equal.
    pub(crate) fn number_targets(a: &NodeItem, b: &NodeItem) -> std::cmp::Ordering {
        a.ag_nexus_count()
            .cmp(&b.ag_nexus_count())
            .then_with(|| a.ag_preferred().cmp(&b.ag_preferred()).reverse())
            .then_with(|| {
                a.node_wrapper()
                    .nexus_count()
                    .cmp(&b.node_wrapper().nexus_count())
            })
    }
}

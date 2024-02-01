#![allow(clippy::field_reassign_with_default)]

use super::super::RestClient;
use std::convert::{TryFrom, TryInto};

pub use stor_port::{
    transport_api,
    types::v0::{
        openapi::{apis, apis::actix_server::RestError, models, tower::client},
        store::pool::PoolLabel,
        transport::{
            AddNexusChild, BlockDevice, Child, ChildUri, CreateNexus, CreatePool, CreateReplica,
            CreateVolume, DestroyNexus, DestroyPool, DestroyReplica, DestroyVolume, EditPool,
            Filter, GetBlockDevices, JsonGrpcRequest, Nexus, NexusId, NexusShareProtocol, Node,
            NodeId, Pool, PoolDeviceUri, PoolId, Protocol, RemoveNexusChild, Replica, ReplicaId,
            ReplicaShareProtocol, ShareNexus, ShareReplica, Specs, Topology, UnshareNexus,
            UnshareReplica, VolumeId, VolumeLabels, VolumePolicy, Watch, WatchCallback,
            WatchResourceId,
        },
    },
};

pub use models::rest_json_error::Kind as RestJsonErrorKind;
use stor_port::{IntoOption, IntoVec};

use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use stor_port::{
    transport_api::ReplyError,
    types::v0::{
        openapi::models::RestJsonError,
        transport::{AffinityGroup, CreateSnapshotVolume, HostNqn, HostNqnParseError, SnapshotId},
    },
};

/// Create Replica Body JSON.
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct CreateReplicaBody {
    /// Size of the replica in bytes.
    pub size: u64,
    /// Thin provisioning.
    pub thin: bool,
    /// Protocol to expose the replica over.
    pub share: Protocol,
    /// Host nqn's allowed to connect to the target.
    pub allowed_hosts: Vec<HostNqn>,
}

impl TryFrom<models::CreateReplicaBody> for CreateReplicaBody {
    type Error = RestError<RestJsonError>;
    fn try_from(src: models::CreateReplicaBody) -> Result<Self, Self::Error> {
        Ok(Self {
            size: src.size,
            thin: src.thin,
            share: match src.share {
                None => Protocol::None,
                Some(models::ReplicaShareProtocol::Nvmf) => Protocol::Nvmf,
            },
            allowed_hosts: src
                .allowed_hosts
                .unwrap_or_default()
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>, HostNqnParseError>>()
                .map_err(ReplyError::from)?,
        })
    }
}

/// Create Pool Body JSON
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct CreatePoolBody {
    /// disk device paths or URIs to be claimed by the pool
    pub disks: Vec<PoolDeviceUri>,
    /// labels to be set on the pool
    pub labels: Option<PoolLabel>,
}
impl From<models::CreatePoolBody> for CreatePoolBody {
    fn from(src: models::CreatePoolBody) -> Self {
        Self {
            disks: src.disks.iter().cloned().map(From::from).collect(),
            labels: src.labels,
        }
    }
}
impl From<CreatePool> for CreatePoolBody {
    fn from(create: CreatePool) -> Self {
        CreatePoolBody {
            disks: create.disks,
            labels: create.labels,
        }
    }
}
impl CreatePoolBody {
    /// Convert into rpc request type.
    pub fn to_request(&self, node_id: NodeId, pool_id: PoolId) -> CreatePool {
        CreatePool {
            node: node_id,
            id: pool_id,
            disks: self.disks.clone(),
            labels: self.labels.clone(),
        }
    }
}
/// Create Pool Body JSON
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct EditPoolBody {
    /// labels to be set on the pool
    pub labels: PoolLabel,
}
impl From<models::EditPoolBody> for EditPoolBody {
    fn from(src: models::EditPoolBody) -> Self {
        Self { labels: src.labels }
    }
}
impl From<EditPool> for EditPoolBody {
    fn from(edit: EditPool) -> Self {
        EditPoolBody {
            labels: edit.labels.unwrap(),
        }
    }
}
impl EditPoolBody {
    /// Convert into rpc request type.
    pub fn to_request(&self, pool_id: PoolId) -> EditPool {
        EditPool {
            id: pool_id,
            labels: Some(self.labels.clone()),
        }
    }
}

impl From<CreateReplica> for CreateReplicaBody {
    fn from(create: CreateReplica) -> Self {
        CreateReplicaBody {
            size: create.size,
            thin: create.thin,
            share: create.share,
            allowed_hosts: create.allowed_hosts.into_vec(),
        }
    }
}
impl CreateReplicaBody {
    /// Convert into rpc request type.
    pub fn to_request(self, node_id: NodeId, pool_id: PoolId, uuid: ReplicaId) -> CreateReplica {
        CreateReplica {
            node: node_id,
            name: None,
            uuid,
            pool_id,
            pool_uuid: None,
            size: self.size,
            thin: self.thin,
            share: self.share,
            managed: false,
            owners: Default::default(),
            allowed_hosts: self.allowed_hosts,
            kind: None,
        }
    }
}

/// Create Nexus Body JSON
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct CreateNexusBody {
    /// size of the device in bytes
    pub size: u64,
    /// replica can be iscsi and nvmf remote targets or a local spdk bdev
    /// (i.e. bdev:///name-of-the-bdev).
    ///
    /// uris to the targets we connect to
    pub children: Vec<ChildUri>,
}
impl From<CreateNexus> for CreateNexusBody {
    fn from(create: CreateNexus) -> Self {
        Self {
            size: create.size,
            children: create.children.into_vec(),
        }
    }
}
impl From<models::CreateNexusBody> for CreateNexusBody {
    fn from(src: models::CreateNexusBody) -> Self {
        Self {
            size: src.size,
            children: src.children.into_iter().map(From::from).collect(),
        }
    }
}
impl CreateNexusBody {
    /// Convert into rpc request type.
    pub fn to_request(&self, node_id: NodeId, nexus_id: NexusId) -> CreateNexus {
        CreateNexus {
            node: node_id,
            uuid: nexus_id,
            size: self.size,
            children: self.children.clone().into_vec(),
            managed: false,
            owner: None,
            config: None,
        }
    }
}

/// Create Volume Body JSON
#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct CreateVolumeBody {
    /// size of the volume in bytes
    pub size: u64,
    /// number of storage replicas
    pub replicas: u64,
    /// Volume policy used to determine if and how to replace a replica
    pub policy: VolumePolicy,
    /// Volume topology used to determine how to place/distribute the data
    pub topology: Option<Topology>,
    /// Volume labels, used ot store custom volume information
    pub labels: Option<VolumeLabels>,
    /// Flag indicating whether the volume should be thin provisioned
    pub thin: bool,
    /// Affinity Group related information.
    pub affinity_group: Option<AffinityGroup>,
}
impl From<models::CreateVolumeBody> for CreateVolumeBody {
    fn from(src: models::CreateVolumeBody) -> Self {
        Self {
            size: src.size,
            replicas: src.replicas as u64,
            policy: src.policy.into(),
            topology: src.topology.into_opt(),
            labels: src.labels,
            thin: src.thin,
            affinity_group: src.affinity_group.map(|ag| ag.into()),
        }
    }
}
impl From<CreateVolume> for CreateVolumeBody {
    fn from(create: CreateVolume) -> Self {
        CreateVolumeBody {
            size: create.size,
            replicas: create.replicas,
            policy: create.policy,
            topology: create.topology,
            labels: create.labels,
            thin: create.thin,
            affinity_group: create.affinity_group,
        }
    }
}
impl CreateVolumeBody {
    /// Convert into rpc request type.
    pub fn to_create_volume(&self, volume_id: VolumeId) -> CreateVolume {
        CreateVolume {
            uuid: volume_id,
            size: self.size,
            replicas: self.replicas,
            policy: self.policy.clone(),
            topology: self.topology.clone(),
            labels: self.labels.clone(),
            thin: self.thin,
            affinity_group: self.affinity_group.clone(),
            cluster_capacity_limit: None,
        }
    }
    /// Convert into rpc request type.
    pub fn to_create_snapshot_volume(
        &self,
        snapshot_id: SnapshotId,
        volume_id: VolumeId,
    ) -> CreateSnapshotVolume {
        CreateSnapshotVolume::new(snapshot_id, self.to_create_volume(volume_id))
    }
}

impl RestClient {
    /// Get Autogenerated Openapi client v0
    pub fn v00(&self) -> client::direct::ApiClient {
        self.openapi_client_v0.clone()
    }
}

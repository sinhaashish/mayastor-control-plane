use super::*;
use grpc::operations::pool::traits::PoolOperations;
use stor_port::types::v0::transport::{DestroyPool, Filter};
use transport_api::{ReplyError, ReplyErrorKind, ResourceKind};

fn client() -> impl PoolOperations {
    core_grpc().pool()
}

async fn destroy_pool(filter: Filter) -> Result<(), RestError<RestJsonError>> {
    let destroy = match filter.clone() {
        Filter::NodePool(node_id, pool_id) => DestroyPool {
            node: node_id,
            id: pool_id,
        },
        Filter::Pool(pool_id) => {
            let node_id = match client().get(filter, None).await {
                Ok(pools) => pool(pool_id.to_string(), pools.into_inner().get(0))?.node(),
                Err(error) => return Err(RestError::from(error)),
            };
            DestroyPool {
                node: node_id,
                id: pool_id,
            }
        }
        _ => {
            return Err(RestError::from(ReplyError {
                kind: ReplyErrorKind::Internal,
                resource: ResourceKind::Pool,
                source: "destroy_pool".to_string(),
                extra: "invalid filter for resource".to_string(),
            }))
        }
    };
    client().destroy(&destroy, None).await?;
    Ok(())
}

#[async_trait::async_trait]
impl apis::actix_server::Pools for RestApi {
    async fn del_node_pool(
        Path((node_id, pool_id)): Path<(String, String)>,
    ) -> Result<(), RestError<RestJsonError>> {
        destroy_pool(Filter::NodePool(node_id.into(), pool_id.into())).await
    }

    async fn del_pool(Path(pool_id): Path<String>) -> Result<(), RestError<RestJsonError>> {
        destroy_pool(Filter::Pool(pool_id.into())).await
    }

    async fn get_node_pool(
        Path((node_id, pool_id)): Path<(String, String)>,
    ) -> Result<models::Pool, RestError<RestJsonError>> {
        let pool = pool(
            pool_id.clone(),
            client()
                .get(Filter::NodePool(node_id.into(), pool_id.into()), None)
                .await?
                .into_inner()
                .get(0),
        )?;
        Ok(pool.into())
    }

    async fn get_node_pools(
        Path(id): Path<String>,
    ) -> Result<Vec<models::Pool>, RestError<RestJsonError>> {
        let pools = client().get(Filter::Node(id.into()), None).await?;
        Ok(pools.into_inner().into_iter().map(From::from).collect())
    }

    async fn get_pool(
        Path(pool_id): Path<String>,
    ) -> Result<models::Pool, RestError<RestJsonError>> {
        let pool = pool(
            pool_id.clone(),
            client()
                .get(Filter::Pool(pool_id.clone().into()), None)
                .await?
                .into_inner()
                .get(0),
        )?;
        Ok(pool.into())
    }

    async fn get_pools() -> Result<Vec<models::Pool>, RestError<RestJsonError>> {
        let pools = client().get(Filter::None, None).await?;
        Ok(pools.into_inner().into_iter().map(From::from).collect())
    }

    async fn patch_node_pool(
        Path(pool_id): Path<String>,
        Body(edit_pool_body): Body<models::EditPoolBody>,
    ) -> Result<models::Pool, RestError<RestJsonError>> {
        let edit = EditPoolBody::from(edit_pool_body).to_request(pool_id.into());
        let pool = client().patch(&edit, None).await?;
        Ok(pool.into())
    }

    async fn put_node_pool(
        Path((node_id, pool_id)): Path<(String, String)>,
        Body(create_pool_body): Body<models::CreatePoolBody>,
    ) -> Result<models::Pool, RestError<RestJsonError>> {
        let create =
            CreatePoolBody::from(create_pool_body).to_request(node_id.into(), pool_id.into());
        let pool = client().create(&create, None).await?;
        Ok(pool.into())
    }
}

/// returns pool from pool option and returns an error on non existence
pub fn pool(pool_id: String, pool: Option<&Pool>) -> Result<Pool, ReplyError> {
    match pool {
        Some(pool) => Ok(pool.clone()),
        None => Err(ReplyError {
            kind: ReplyErrorKind::NotFound,
            resource: ResourceKind::Pool,
            source: "Requested pool was not found".to_string(),
            extra: format!("Pool id : {pool_id}"),
        }),
    }
}

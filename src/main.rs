use std::convert::Infallible;

use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig},
    EmptyMutation, EmptySubscription, Object, Schema,
};
use async_graphql_warp::{graphql_subscription, Response};
use warp::http::Response as HttpResponse;
use warp::Filter;

pub mod data;
pub mod logic;
pub mod schema;
pub mod utils;

use schema::QueryRoot;

use crate::{
    data::Storage,
    schema::{MutationRoot, Subscription},
};

#[tokio::main]
async fn main() {
    pretty_env_logger::init();
    let schema = Schema::build(QueryRoot, MutationRoot, Subscription)
        .data(Storage::default())
        .finish();

    log::info!("Playground: http://localhost:8000");

    let graphql_post = async_graphql_warp::graphql(schema.clone()).and_then(
        |(schema, request): (
            Schema<QueryRoot, MutationRoot, Subscription>,
            async_graphql::Request,
        )| async move { Ok::<_, Infallible>(Response::from(schema.execute(request).await)) },
    );

    let graphql_playground = warp::path::end().and(warp::get()).map(|| {
        HttpResponse::builder()
            .header("content-type", "text/html")
            .body(playground_source(
                GraphQLPlaygroundConfig::new("/").subscription_endpoint("/"),
            ))
    });

    let routes = graphql_subscription(schema)
        .or(graphql_playground)
        .or(graphql_post);

    warp::serve(routes).run(([0, 0, 0, 0], 8000)).await;
}

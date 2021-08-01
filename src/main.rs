use std::convert::Infallible;

use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig},
    Schema,
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
        .or(graphql_post)
        .with(
            warp::cors()
                .allow_any_origin()
                .allow_methods(vec!["GET", "POST", "DELETE", "OPTIONS"])
                .allow_headers(vec!["Content-Type"])
                .build(),
        );

    warp::serve(routes)
        .run((
            [0, 0, 0, 0],
            std::env::var("PORT")
                .unwrap_or("8000".into())
                .parse()
                .unwrap_or(8000),
        ))
        .await;
}

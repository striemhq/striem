//! Feature flag middleware for API responses.
//!
//! Adds X-Feature-Flag header to all responses to communicate
//! enabled features to the frontend.

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::ApiState;

/// Middleware to add feature flags to response headers.
///
/// # Usage
/// ```no_run
/// use axum::{Router, middleware};
///
/// let app = Router::new()
///     .layer(middleware::from_fn_with_state(state, feature_flag_middleware));
/// ```
pub(crate) async fn feature_flag_middleware(
    State(state): State<ApiState>,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .append("X-Feature-Flag", state.features.clone());
    response
}

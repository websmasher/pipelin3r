use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};

use crate::AppError;

/// Custom JSON extractor that returns 400 (Bad Request) instead of Axum's
/// default 422 (Unprocessable Entity) for deserialization failures.
///
/// Use `ValidatedJson<T>` instead of `Json<T>` in handler signatures to
/// ensure a consistent error contract across all endpoints.
#[derive(Debug)]
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(Self(value)),
            Err(e) => Err(AppError::BadRequest(e.body_text())),
        }
    }
}

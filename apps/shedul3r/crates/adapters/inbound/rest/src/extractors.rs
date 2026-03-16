use std::future::Future;
use std::pin::Pin;

use actix_web::FromRequest;
use actix_web::HttpRequest;
use actix_web::dev::Payload;
use actix_web::web::Json;

use crate::AppError;

/// Custom JSON extractor that returns 400 (Bad Request) instead of actix-web's
/// default error for deserialization failures.
///
/// Use `ValidatedJson<T>` instead of `Json<T>` in handler signatures to
/// ensure a consistent error contract across all endpoints.
#[derive(Debug)]
pub struct ValidatedJson<T>(pub T);

impl<T> FromRequest for ValidatedJson<T>
where
    T: serde::de::DeserializeOwned + 'static,
{
    type Error = AppError;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let json_fut = Json::<T>::from_request(req, payload);
        Box::pin(async move {
            match json_fut.await {
                Ok(Json(value)) => Ok(Self(value)),
                Err(e) => Err(AppError::BadRequest(e.to_string())),
            }
        })
    }
}

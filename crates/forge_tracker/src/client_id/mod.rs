#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
mod generic;

#[cfg(target_os = "android")]
pub use android::get_or_create_client_id;
#[cfg(not(target_os = "android"))]
pub use generic::get_or_create_client_id;

pub const DEFAULT_CLIENT_ID: &str = "<anonymous>";

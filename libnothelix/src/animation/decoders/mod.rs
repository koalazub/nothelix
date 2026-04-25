#[cfg(feature = "gif")]
pub mod gif;

#[cfg(feature = "apng")]
pub mod apng;

#[cfg(feature = "webp")]
pub mod webp;

#[cfg(feature = "video")]
pub mod mp4;

#[cfg(feature = "video")]
pub mod webm;

#[cfg(feature = "lottie")]
pub mod lottie;

#[cfg(test)]
pub mod gif_fixture;

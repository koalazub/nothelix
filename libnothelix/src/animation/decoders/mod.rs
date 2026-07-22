mod frames;

#[cfg(feature = "gif")]
pub mod gif;

#[cfg(feature = "apng")]
pub mod apng;

#[cfg(feature = "webp")]
pub mod webp;

#[cfg(any(feature = "video", feature = "lottie"))]
pub mod unsupported;

#[cfg(test)]
pub mod gif_fixture;

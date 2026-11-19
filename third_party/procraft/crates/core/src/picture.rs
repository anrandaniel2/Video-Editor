//! Pictures the app ships as lossless JPEG XL for the interface to draw. Android's own decoders
//! cannot read JPEG XL (no platform support, and no Java library that is not as big as the format's
//! savings), so the native library decodes it and the interface turns the pixels into a bitmap.

/// A picture decoded for the interface: `rgba` is four bytes a pixel, row by row from the top.
#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct Picture {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes: red, green, blue, alpha.
    pub rgba: Vec<u8>,
}

/// Decode a lossless JPEG XL picture (see [`Picture`]); `None` when the bytes are not one.
#[uniffi::export]
pub fn decode_picture_jxl(bytes: Vec<u8>) -> Option<Picture> {
    let (width, height, rgba) = procraft_engine::environment::decode_rgba(&bytes)?;
    Some(Picture { width, height, rgba })
}

//! Decoding and encoding of BMP images.
//!
//! Used as the standard bitmap storage format in the Microsoft Windows environment. Although it
//! is based on Windows internal bitmap data structures, it is supported by many non-Windows and
//! non-PC applications.
//!
//! # Related Links
//! * http://www.fileformat.info/format/bmp/egff.htm - Microsoft Windows Bitmap File Format Summary
//!

pub use self::encoder::BMPEncoder;
pub use self::decoder::BMPDecoder;

mod encoder;
mod decoder;

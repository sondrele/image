use std::io;
use std::io::Write;
use std::num::Float;
use std::ops::{Deref, DerefMut};

use byteorder::{WriteBytesExt, LittleEndian};

use color::Rgb;

use buffer::ImageBuffer;

/// A BMP encoder.
///
/// It supports encoding of RGB8 (24-bit), uncompressed BMP Version 3 images.
///
pub struct BMPEncoder<Image> {
    image: Image,
}

impl<Container> BMPEncoder<ImageBuffer<Rgb<u8>, Container>>
where Container: Deref<Target=[u8]> + DerefMut {
    /// Creates a new BMP encoder.
    pub fn new(image: ImageBuffer<Rgb<u8>, Container>) -> BMPEncoder<ImageBuffer<Rgb<u8>, Container>> {
        BMPEncoder {
            image: image,
        }
    }

    /// Encodes an image from the internal image buffer.
    pub fn encode<W: Write>(&mut self, w: &mut W) -> io::Result<()> {
        let width = self.image.width();
        let height = self.image.height();
        let bpp = 24;

        let header_size = 2 + 12 + 40; // magic numbers + bmp header size + dib header size
        let row_size = ((bpp as f32 * width as f32 + 31.0) / 32.0).floor() as u32 * 4;
        let data_size = row_size * height; // (width + padding) * height

        try!(self.write_header(w, header_size, data_size, width as i32, height as i32));
        try!(self.write_data(w, width, height));
        Ok(())
    }

    fn write_header<W: Write>(&mut self, w: &mut W, header_size: u32, data_size: u32,
                    width: i32, height: i32) -> io::Result<()> {
        // Magic numbers
        try!(w.write_all(b"BM"));

        // BMP header
        try!(w.write_u32::<LittleEndian>(header_size + data_size)); // file_size
        try!(w.write_u16::<LittleEndian>(0));                       // Creator1: always 0
        try!(w.write_u16::<LittleEndian>(0));                       // Creator2: always 0
        try!(w.write_u32::<LittleEndian>(header_size));             // pixel offset

        // DIB header
        try!(w.write_u32::<LittleEndian>(40));                      // dib header size
        try!(w.write_i32::<LittleEndian>(width));                   // width
        try!(w.write_i32::<LittleEndian>(height));                  // height
        try!(w.write_u16::<LittleEndian>(1));                       // #planes: always 1
        try!(w.write_u16::<LittleEndian>(24));                      // bits per pixel
        try!(w.write_u32::<LittleEndian>(0));                       // compression type: uncompressed
        try!(w.write_u32::<LittleEndian>(data_size));               // dib data size
        try!(w.write_i32::<LittleEndian>(1000));                    // horizontal resolution in pixels/m
        try!(w.write_i32::<LittleEndian>(1000));                    // vertical resolution in pixels/m
        try!(w.write_u32::<LittleEndian>(0));                       // #colors in image palette: 0
        try!(w.write_u32::<LittleEndian>(0));                       // #imporant colors in image palette
        Ok(())
    }

    fn write_data<W: Write>(&mut self, w: &mut W, width: u32, height: u32) -> io::Result<()> {
        let padding_len = width % 4;
        let padding = &[0; 4][0 .. padding_len as usize];
        for y in 0 .. height {
            for x in 0 .. width {
                let px = &self.image[(x, y)];
                try!(w.write_all(&[px[2], px[1], px[0]]));
            }
            try!(w.write_all(padding));
        }
        Ok(())
    }
}

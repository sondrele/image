use std::num::SignedInt;
use std::old_io;
use std::old_io::{Seek, SeekCur, SeekSet};

use color;

use image::ImageDecoder;
use image::DecodingResult;
use image::ImageResult;
use image::ImageError;
use image::ImageError::{FormatError, UnsupportedError};

enum State {
    Start,
    HaveBmpHeader,
    HaveDibHeader,
}

struct BMPVersion3Header {
    header_size:      u32,
    width:            i32,
    height:           i32,
    planes:           u16,
    bits_per_pixel:   u16,
    compression:      u32,
    bitmap_size:      u32,
    horz_resolution:  i32,
    vert_resolution:  i32,
    colors_used:      u32,
    colors_important: u32,
}

impl BMPVersion3Header {
    fn new() -> BMPVersion3Header {
        BMPVersion3Header {
            header_size:      0,
            width:            0,
            height:           0,
            planes:           0,
            bits_per_pixel:   0,
            compression:      0,
            bitmap_size:      0,
            horz_resolution:  0,
            vert_resolution:  0,
            colors_used:      0,
            colors_important: 0,
        }
    }
}

/// A BMP decoder.
pub struct BMPDecoder<R: Reader + Seek> {
    r: R,
    state: State,

    file_size: u32,
    pixel_offset: u32,
    width: u32,
    height: u32,
    header: BMPVersion3Header,
}

impl<R: Reader + Seek> BMPDecoder<R> {
    /// Creates a new BMP Decoder wrapped in an `ImageResult`.
    ///
    /// The BMP decoder requires a `Reader` that also implements the `Seek` trait.
    pub fn new(r: R) -> ImageResult<BMPDecoder<R>> {
        let decoder = BMPDecoder {
            r: r,
            state: State::Start,

            file_size: 0,
            pixel_offset: 0,
            width: 0,
            height: 0,
            header: BMPVersion3Header::new(),
        };
        Ok(decoder)
    }

    fn read_bmp_header(&mut self) -> ImageResult<()> {
        match self.state {
            State::Start => {
                let mut magic_numbers = [0; 2];
                try!(self.r.read_at_least(2, &mut magic_numbers));

                if magic_numbers != b"BM" {
                    return Err(FormatError("BMP signature not found".to_string()));
                }

                let file_size = try!(self.r.read_le_u32());
                let _ = try!(self.r.read_le_u16()); // creator1
                let _ = try!(self.r.read_le_u16()); // creator2
                let pixel_offset = try!(self.r.read_le_u32());

                self.file_size = file_size;
                self.pixel_offset = pixel_offset;
                self.state = State::HaveBmpHeader;
                Ok(())
            }
            _ =>  { Ok(()) }
        }
    }

    fn read_dib_header(&mut self) -> ImageResult<()> {
        match self.state {
            State::Start => try!(self.read_bmp_header()),
            State::HaveBmpHeader => (),
            State::HaveDibHeader => return Ok(())
        }

        let dib = BMPVersion3Header {
            header_size:      try!(self.r.read_le_u32()),
            width:            try!(self.r.read_le_i32()),
            height:           try!(self.r.read_le_i32()),
            planes:           try!(self.r.read_le_u16()),
            bits_per_pixel:   try!(self.r.read_le_u16()),
            compression:      try!(self.r.read_le_u32()),
            bitmap_size:      try!(self.r.read_le_u32()),
            horz_resolution:  try!(self.r.read_le_i32()),
            vert_resolution:  try!(self.r.read_le_i32()),
            colors_used:      try!(self.r.read_le_u32()),
            colors_important: try!(self.r.read_le_u32()),
        };

        match dib.header_size {
            // BMPv2 has a header size of 12 bytes
            12 => return Err(
                UnsupportedError("BMP Version 2 is not supported".to_string())
            ),
            // BMPv3 has a header size of 40 bytes, it is NT if the compression type is 3
            40 if dib.compression == 3 => return Err(
                UnsupportedError("BMP Version 3NT is not supported".to_string())
            ),
            // BMPv4 has more data in its header, it is currently ignored but we still try to parse it
            108 | _ => ()
        }

        match dib.bits_per_pixel {
            // Currently supported
            24 => (),
            other => return Err(
                UnsupportedError(format!("Unsupported bits per pixel: {}", other))
            )
        }

        match dib.compression {
            0 => (),
            other => return Err(
                UnsupportedError(format!("Unsupported compression type: {}", other))
            ),
        }

        self.header = dib;
        self.width = self.header.width.abs() as u32;
        self.height = self.header.height.abs() as u32;
        self.state = State::HaveDibHeader;
        Ok(())
    }

    fn read_pixels(&mut self) -> ImageResult<Vec<u8>> {
        try!(self.read_dib_header());

        let mut data = Vec::with_capacity(self.height as usize * self.width as usize);
        let padding = self.width as i64 % 4;
        // seek until data
        try!(self.r.seek(self.pixel_offset as i64, SeekSet));
        // read pixels until padding
        let mut px = [0; 3];
        for _ in 0 .. self.height {
            for _ in 0 .. self.width {
                try!(self.r.read(&mut px));
                data.push_all(&[px[2], px[1], px[0]]);
            }
            // seek padding
            try!(self.r.seek(padding, SeekCur));
        }
        Ok(data)
    }
}

impl<R: Reader + Seek> ImageDecoder for BMPDecoder<R> {
    fn dimensions(&mut self) -> ImageResult<(u32, u32)> {
        let _ = try!(self.read_dib_header());

        return Ok((self.width, self.height));
    }

    fn colortype(&mut self) -> ImageResult<color::ColorType> {
        let _ = try!(self.read_dib_header());

        match self.header.bits_per_pixel {
            24 => Ok(color::ColorType::RGB(8)),
            other => Err(ImageError::UnsupportedColor(color::ColorType::RGB(other as u8)))
        }
    }

    fn row_len(&mut self) -> ImageResult<usize> {
        let _ = try!(self.read_dib_header());

        Ok(3 * self.width as usize)
    }

    fn read_scanline(&mut self, _: &mut [u8]) -> ImageResult<u32> {
        unimplemented!()
    }

    fn read_image(&mut self) -> ImageResult<DecodingResult> {
        let img = try!(self.read_pixels());

        Ok(DecodingResult::U8(img))
    }
}

#[cfg(test)]
mod tests {
    use {open, Rgb};
    use buffer::Pixel;

    #[test]
    fn test_read_bmp_image_coordinates() {
        let img = open(&Path::new("tests/images/bmp/rgbw.bmp")).unwrap();
        let rgb = img.as_rgb8().unwrap();

        assert_eq!(rgb.dimensions(), (2, 2));
    }

    #[test]
    fn test_read_bmp_image_data() {
        let img = open(&Path::new("tests/images/bmp/rgbw.bmp")).unwrap();

        let rgb = img.as_rgb8().unwrap();
        assert_eq!(rgb.get_pixel(0, 1).channels(), [255, 0, 0]);
        assert_eq!(rgb.get_pixel(1, 1).channels(), [0, 255, 0]);
        assert_eq!(rgb.get_pixel(0, 0).channels(), [0, 0, 255]);
        assert_eq!(rgb.get_pixel(1, 0).channels(), [255, 255, 255]);
    }
}

//! This modules provides an GIF en-/decoder
//!


// A very good resource for the file format is
// http://giflib.sourceforge.net/whatsinagif/bits_and_bytes.html

use std::io::{self, Read};
use byteorder::{ReadBytesExt, LittleEndian};
use std::num::FromPrimitive;

use num::rational::Ratio;
use imageops::overlay;

use color;
use animation::Frame;
use image::{ImageError, ImageResult, DecodingResult, ImageDecoder};
use buffer::{ImageBuffer, GrayImage, RgbaImage};

use utils::lzw;
use utils::bitstream::{LsbReader};

#[derive(PartialEq)]
enum State {
    Start,
    HaveHeader,
    HaveLSD,
}

/// A gif decoder
pub struct GIFDecoder<R: Read> {
    r: R,
    state: State,

    width: u16,
    height: u16,
    global_table: Vec<(u8, u8, u8)>,
    global_background_index: Option<u8>,
    delay: u16,
    local_transparent_index: Option<u8>,
}

impl<R: Read> GIFDecoder<R> {
    /// Creates a new GIF decoder
    pub fn new(r: R) -> GIFDecoder<R> {
        GIFDecoder {
            r: r,
            state: State::Start,

            width: 0,
            height: 0,
            global_table: Vec::new(),
            global_background_index: None,
            delay: 0,
            local_transparent_index: None,
        }
    }

    fn read_header(&mut self) -> ImageResult<()> {
        if self.state == State::Start {
            let mut signature = [0; 3];
            if try!(self.r.read(&mut signature)) != 3 {
                return Err(ImageError::ImageEnd);
            }

            let mut version = [0; 3];
            if try!(self.r.read(&mut version)) != 3 {
                return Err(ImageError::ImageEnd);
            }

            if signature != b"GIF"[..] {
                Err(ImageError::FormatError("GIF signature not found.".to_string()))
            } else if version != b"87a"[..] && version != b"89a"[..] {
                Err(ImageError::UnsupportedError(
                    format!("GIF version {:?} is not supported.", version)
                ))
            } else {
                self.state = State::HaveHeader;
                Ok(())
            }
        } else { Ok(()) }
    }

    fn read_logical_screen_descriptor(&mut self) -> ImageResult<()> {
        try!(self.read_header());
        if self.state == State::HaveHeader {
            self.width  = try!(self.r.read_u16::<LittleEndian>());
            self.height = try!(self.r.read_u16::<LittleEndian>());

            let fields = try!(self.r.read_u8());

            let global_table = fields & 0x80 != 0;

            let entries = if global_table {
                1 << ((fields & 0b111) + 1) as usize
            } else {
                0usize
            };

            let b = try!(self.r.read_u8());
            if global_table {
                self.global_table.reserve_exact(entries);
                self.global_background_index = Some(b);
            }

            let _aspect_ratio = try!(self.r.read_u8());

            let mut buf = Vec::with_capacity(3 * entries);
            try!(self.r.by_ref().take(3 * entries as u64).read_to_end(&mut buf));

            for rgb in buf.chunks(3) {
                self.global_table.push((rgb[0], rgb[1], rgb[2]));
            }
            self.state = State::HaveLSD;
            Ok(())
        } else { Ok(()) }
    }

    fn read_extension(&mut self) -> ImageResult<()> {
        use super::Extension::{Text, Control, Comment, Application};

        match FromPrimitive::from_u8(try!(self.r.read_u8())) {
            Some(Text) => try!(self.skip_extension()),
            Some(Control) => try!(self.read_control_extension()),
            Some(Comment) => try!(self.skip_extension()),
            Some(Application) => try!(self.skip_extension()),
            None => try!(self.skip_extension())
        }
        Ok(())
    }

    fn read_control_extension(&mut self) -> ImageResult<()> {
        let size = try!(self.r.read_u8());
        if size != 4 {
            return Err(ImageError::FormatError(
                "Malformed graphics control extension.".to_string()
            ))
        }
        let fields = try!(self.r.read_u8());
        self.delay = try!(self.r.read_u16::<LittleEndian>());
        let trans  = try!(self.r.read_u8());

        if fields & 1 != 0 {
            self.local_transparent_index = Some(trans);
        }
        let size = try!(self.r.read_u8());
        if size != 0 {
            return Err(ImageError::FormatError(
                "Malformed graphics control extension.".to_string()
            ))
        }
        Ok(())
    }

    /// Skips an unknown extension
    fn skip_extension(&mut self) -> ImageResult<()> {
        let mut size = try!(self.r.read_u8());
        while size != 0 {
            for _ in (0..size) {
                let _ = try!(self.r.read_u8());
            }
            size = try!(self.r.read_u8());
        }
        Ok(())
    }

    /// Reads data blocks
    fn read_data(&mut self) -> ImageResult<Vec<u8>> {
        let mut size = try!(self.r.read_u8()) as usize;
        let mut data = Vec::with_capacity(size);
        while size != 0 {
            try!(self.r.by_ref().take(size as u64).read_to_end(&mut data));
            size = try!(self.r.read_u8()) as usize;
        }
        Ok(data)
    }

    #[allow(unused_variables)]
    fn read_frame(&mut self) -> ImageResult<Frame> {
        let image_left   = try!(self.r.read_u16::<LittleEndian>());
        let image_top    = try!(self.r.read_u16::<LittleEndian>());
        let image_width  = try!(self.r.read_u16::<LittleEndian>());
        let image_height = try!(self.r.read_u16::<LittleEndian>());

        let fields = try!(self.r.read_u8());

        let local_table = (fields & 0b1000_0000) != 0;
        let interlace   = (fields & 0b0100_0000) != 0;
        let table_size  =  fields & 0b0000_0111;

        if interlace {
            return Err(ImageError::UnsupportedError(
                "Interlaced images are not supported.".to_string()
            ))
        }

        let local_table = if local_table {
            let entries = 1 << (table_size + 1) as usize;
            let mut table = Vec::with_capacity(entries * 3);
            let mut buf = Vec::with_capacity(3 * entries);
            try!(self.r.by_ref().take(3 * entries as u64).read_to_end(&mut buf));

            for rgb in buf.chunks(3) {
                table.push((rgb[0], rgb[1], rgb[2]));
            }
            Some(table)
        } else {
            None
        };

        let code_size = try!(self.r.read_u8());
        let data = try!(self.read_data());

        let mut indices = Vec::with_capacity(
            image_width as usize
            * image_height as usize
        );
        try!(lzw::decode(
            LsbReader::new(io::Cursor::new(data)),
            &mut indices,
            code_size
        ));

        let table = if let Some(ref table) = local_table {
            table
        } else {
            &self.global_table
        };

        let image: Option<GrayImage> = ImageBuffer::from_vec(
            image_width as u32,
            image_height as u32,
            indices
        );
        if let Some(image) = image {
            let image = image.expand_palette(table, self.local_transparent_index);
            Ok(Frame::from_parts(
                image,
                image_left as u32,
                image_top as u32,
                Ratio::new(self.delay, 100)
            ))
        } else {
            Err(ImageError::FormatError(
                "Image data has not the expected size.".to_string()
            ))
        }
    }

    fn next_frame(&mut self) -> ImageResult<Option<Frame>> {
        use super::Block::{Image, Extension, Trailer};

        try!(self.read_logical_screen_descriptor());
        loop {
            match FromPrimitive::from_u8(try!(self.r.read_u8())) {
                Some(Extension) => try!(self.read_extension()),
                Some(Image) => return self.read_frame().map(|v| Some(v)),
                Some(Trailer) => return Ok(None),
                None => return Err(ImageError::UnsupportedError(
                    "Unknown block encountered".to_string()
                ))
            }
        }
    }
}

impl<R: Read> ImageDecoder for GIFDecoder<R> {
    fn dimensions(&mut self) -> ImageResult<(u32, u32)> {
        let _ = try!(self.read_logical_screen_descriptor());
        Ok((self.width as u32, self.height as u32))
    }

    fn colortype(&mut self) -> ImageResult<color::ColorType> {
        let _ = try!(self.read_logical_screen_descriptor());
        Ok(color::ColorType::RGBA(8))
    }

    fn row_len(&mut self) -> ImageResult<usize> {
        let _ = try!(self.read_logical_screen_descriptor());
        Ok(3 * self.width as usize)
    }

    fn read_scanline(&mut self, _: &mut [u8]) -> ImageResult<u32> {
        unimplemented!()
    }

    fn read_image(&mut self) -> ImageResult<DecodingResult> {
        let (width, height) = try!(self.dimensions());
        let background = if let Some(idx) = self.global_background_index {
            let (r, g, b) = self.global_table[idx as usize];
            color::Rgba([r, g, b, 255])
        } else {
            color::Rgba([0, 0, 0, 255])
        };
        let mut canvas: RgbaImage = ImageBuffer::from_pixel(width, height, background);
        let frame = try!(self.next_frame());
        match frame {
            Some(frame) => {
                let left = frame.left();
                let top = frame.top();
                let buffer = frame.into_buffer();
                overlay(&mut canvas, &buffer, left, top);
                while let Some(frame) = try!(self.next_frame()) {
                    if frame.delay() == Ratio::new(0, 100) {
                        let left = frame.left();
                        let top = frame.top();
                        let buffer = frame.into_buffer();
                        overlay(&mut canvas, &buffer, left, top);
                    } else {
                        break
                    }
                }
                Ok(DecodingResult::U8(canvas.into_raw()))
            },
            None => Err(ImageError::ImageEnd)
        }
    }
}

//! Bounded JPEG/PNG intake and metadata-removing normalization.

use std::{
    io::{self, Cursor, Write},
    num::NonZeroU32,
};

use image::{ExtendedColorType, ImageFormat, ImageReader, Limits, codecs::jpeg::JpegEncoder};
use thiserror::Error;

pub const MAX_IMAGE_INPUT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_IMAGE_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 25_000_000;
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_DECODER_ALLOCATION_BYTES: u64 = 128 * 1024 * 1024;
const NORMALIZED_JPEG_QUALITY: u8 = 90;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageMediaType {
    Jpeg,
    Png,
}

impl ImageMediaType {
    pub fn parse(content_type: &str) -> Option<Self> {
        match content_type
            .split(';')
            .next()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("image/jpeg") => Some(Self::Jpeg),
            Some("image/png") => Some(Self::Png),
            _ => None,
        }
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Png => ImageFormat::Png,
        }
    }

    fn matches_signature(self, bytes: &[u8]) -> bool {
        match self {
            Self::Jpeg => bytes.starts_with(&[0xff, 0xd8, 0xff]),
            Self::Png => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        }
    }
}

#[derive(Debug)]
pub struct NormalizedImage {
    pub bytes: Vec<u8>,
    pub width: NonZeroU32,
    pub height: NonZeroU32,
}

impl NormalizedImage {
    pub const MEDIA_TYPE: &'static str = "image/jpeg";
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ImageIntakeError {
    #[error("the image media type is not supported")]
    UnsupportedMediaType,
    #[error("the image body is empty or exceeds the input limit")]
    InputSize,
    #[error("the image signature does not match its declared media type")]
    Signature,
    #[error("the image could not be decoded")]
    InvalidImage,
    #[error("the decoded image dimensions exceed the allowed limits")]
    Dimensions,
    #[error("the normalized image exceeds the output limit")]
    OutputSize,
}

pub fn normalize_image(
    media_type: ImageMediaType,
    bytes: &[u8],
) -> Result<NormalizedImage, ImageIntakeError> {
    validate_input(media_type, bytes)?;
    let format = media_type.image_format();
    let (width, height) = dimensions(format, bytes)?;
    validate_dimensions(width, height)?;

    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decoder_limits());
    let decoded = reader
        .decode()
        .map_err(|_| ImageIntakeError::InvalidImage)?;
    if decoded.width() != width || decoded.height() != height {
        return Err(ImageIntakeError::InvalidImage);
    }

    let rgb = decoded.into_rgb8();
    let mut output = BoundedWriter::new(MAX_IMAGE_OUTPUT_BYTES, bytes.len());
    let encode_result = JpegEncoder::new_with_quality(&mut output, NORMALIZED_JPEG_QUALITY).encode(
        rgb.as_raw(),
        width,
        height,
        ExtendedColorType::Rgb8,
    );
    if output.exceeded {
        return Err(ImageIntakeError::OutputSize);
    }
    encode_result.map_err(|_| ImageIntakeError::InvalidImage)?;

    Ok(NormalizedImage {
        bytes: output.bytes,
        width: NonZeroU32::new(width).ok_or(ImageIntakeError::Dimensions)?,
        height: NonZeroU32::new(height).ok_or(ImageIntakeError::Dimensions)?,
    })
}

fn validate_input(media_type: ImageMediaType, bytes: &[u8]) -> Result<(), ImageIntakeError> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_INPUT_BYTES {
        return Err(ImageIntakeError::InputSize);
    }
    if !media_type.matches_signature(bytes) {
        return Err(ImageIntakeError::Signature);
    }
    Ok(())
}

fn dimensions(format: ImageFormat, bytes: &[u8]) -> Result<(u32, u32), ImageIntakeError> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decoder_limits());
    reader
        .into_dimensions()
        .map_err(|_| ImageIntakeError::InvalidImage)
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ImageIntakeError> {
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(ImageIntakeError::Dimensions)?;
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || pixels > MAX_IMAGE_PIXELS
    {
        return Err(ImageIntakeError::Dimensions);
    }
    Ok(())
}

fn decoder_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODER_ALLOCATION_BYTES);
    limits
}

struct BoundedWriter {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}

impl BoundedWriter {
    fn new(maximum: usize, expected: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(expected.min(maximum)),
            maximum,
            exceeded: false,
        }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(next_length) = self.bytes.len().checked_add(buffer.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("normalized image limit exceeded"));
        };
        if next_length > self.maximum {
            self.exceeded = true;
            return Err(io::Error::other("normalized image limit exceeded"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use image::{ImageBuffer, ImageEncoder, Rgb, codecs::png::PngEncoder};

    use super::*;

    #[test]
    fn only_declared_jpeg_and_png_media_types_are_accepted() {
        assert_eq!(
            ImageMediaType::parse("image/jpeg"),
            Some(ImageMediaType::Jpeg)
        );
        assert_eq!(
            ImageMediaType::parse(" IMAGE/PNG ; charset=binary"),
            Some(ImageMediaType::Png)
        );
        assert_eq!(ImageMediaType::parse("image/gif"), None);
        assert_eq!(ImageMediaType::parse("application/octet-stream"), None);
    }

    #[test]
    fn declared_media_type_must_match_magic_bytes() {
        let png = png(2, 2);

        assert_eq!(
            normalize_image(ImageMediaType::Jpeg, &png).expect_err("PNG is not JPEG"),
            ImageIntakeError::Signature
        );
    }

    #[test]
    fn invalid_and_oversized_dimensions_are_rejected() {
        assert_eq!(validate_dimensions(0, 1), Err(ImageIntakeError::Dimensions));
        assert_eq!(
            validate_dimensions(5_001, 5_000),
            Err(ImageIntakeError::Dimensions)
        );
        let wide_png = png(MAX_IMAGE_DIMENSION + 1, 1);
        assert!(matches!(
            normalize_image(ImageMediaType::Png, &wide_png),
            Err(ImageIntakeError::InvalidImage | ImageIntakeError::Dimensions)
        ));
    }

    #[test]
    fn normalization_reencodes_to_bounded_jpeg_without_source_metadata() {
        let marker = b"private-workload-metadata";
        let mut source = Vec::new();
        let image = ImageBuffer::from_pixel(3, 2, Rgb([20_u8, 90_u8, 160_u8]));
        let mut encoder = JpegEncoder::new_with_quality(&mut source, 95);
        encoder
            .set_exif_metadata(marker.to_vec())
            .expect("synthetic EXIF metadata");
        encoder.encode_image(&image).expect("synthetic JPEG");

        let normalized = normalize_image(ImageMediaType::Jpeg, &source)
            .expect("the bounded synthetic JPEG is valid");

        assert_eq!(normalized.width.get(), 3);
        assert_eq!(normalized.height.get(), 2);
        assert!(normalized.bytes.starts_with(&[0xff, 0xd8, 0xff]));
        assert!(normalized.bytes.len() <= MAX_IMAGE_OUTPUT_BYTES);
        assert!(
            !normalized
                .bytes
                .windows(marker.len())
                .any(|window| window == marker)
        );
    }

    #[test]
    fn normalized_output_writer_never_crosses_its_limit() {
        let mut writer = BoundedWriter::new(4, 4);
        writer.write_all(b"1234").expect("exact limit");

        assert!(writer.write_all(b"5").is_err());
        assert!(writer.exceeded);
        assert_eq!(writer.bytes, b"1234");
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let pixels = vec![0_u8; width as usize * height as usize * 3];
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(&pixels, width, height, ExtendedColorType::Rgb8)
            .expect("synthetic PNG");
        bytes
    }
}

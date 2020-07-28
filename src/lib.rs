//! A tiny, synchronous `multipart/form-data` writer.
//!
//! This is mostly untested and was not written with quality in mind. It works for the one use case
//! that I need it for :)
//!
//! `FormData::new()` is the entry point for this library.
use std::borrow::Cow;
use std::io::{Cursor, Empty, Read, Result, Write};

#[macro_use]
extern crate v_escape;

mod quote_string {
    new_escape!(QuoteString, "0x22->\\\" || 0x5C->\\\\ || 0x0D->\\\r");
}

/// Generate a random string that can be used as a multipart boundary.
///
/// It uses `rand`'s thread-local random number generator.
#[cfg(feature = "rand")]
pub fn generate_boundary() -> String {
    use rand::RngCore;

    let mut bytes = [0; 12];
    rand::thread_rng().fill_bytes(&mut bytes);

    fn as_u32(slice: &[u8]) -> u32 {
        let mut copy = [0; 4];
        copy.copy_from_slice(slice);
        u32::from_ne_bytes(copy)
    }

    let a = as_u32(&bytes[0..4]);
    let b = as_u32(&bytes[4..8]);
    let c = as_u32(&bytes[8..]);

    format!("--------------------------{:x}{:x}{:x}", a, b, c)
}

/// A multipart/form-data format writer.
#[derive(Debug)]
pub struct FormData<W>
where
    W: Write,
{
    writer: W,
    boundary: String,
}

impl<W> FormData<W>
where
    W: Write,
{
    /// Create a FormData instance that outputs to a [`Write`][].
    ///
    /// This generates a random multipart boundary using `rand`. The `rand` feature must be
    /// enabled to use `FormData::new()`. It is enabled by default.
    #[cfg(feature = "rand")]
    pub fn new(writer: W) -> Self {
        Self::with_boundary(writer, generate_boundary())
    }

    /// Create a FormData instance that outputs to a [`Write`][], with a certain precomputed
    /// multipart boundary string.
    pub fn with_boundary(writer: W, boundary: String) -> Self {
        Self { writer, boundary }
    }

    /// Get the multipart boundary string used by this instance.
    pub fn boundary(&self) -> &str {
        &self.boundary
    }

    /// Get a content type string that you can use in the `Content-Type` header for the request.
    pub fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    /// Append a field to the multipart form body.
    pub fn append<R>(&mut self, mut field: Field<R>) -> Result<()>
    where
        R: Read,
    {
        write!(
            &mut self.writer,
            "--{}\r\nContent-Disposition: form-data; name=\"{}\"",
            self.boundary,
            quote_string::escape(&field.name)
        )?;
        if let Some(filename) = &field.filename {
            write!(&mut self.writer, "; filename=\"{}\"", quote_string::escape(filename))?;
        }
        if let Some(content_type) = &field.content_type {
            write!(&mut self.writer, "\r\nContent-Type: {}", content_type)?;
        }
        write!(&mut self.writer, "\r\n\r\n")?;
        std::io::copy(&mut field.data, &mut self.writer)?;
        write!(&mut self.writer, "\r\n")?;
        Ok(())
    }

    /// Append a text field to the multipart form body.
    pub fn append_text(&mut self, name: &str, data: &str) -> Result<()> {
        let data = Cursor::new(data.as_bytes());
        self.append(FieldBuilder::new(name).build(data))
    }

    /// Append a file field to the multipart form body.
    ///
    /// Provide a file name, mime type, and file contents.
    pub fn append_file(&mut self, name: &str, mime_type: &str, data: &mut impl Read) -> Result<()> {
        let field = FieldBuilder::new(name)
            .filename(name)
            .content_type(mime_type)
            .build(data);
        self.append(field)
    }

    /// Finish the multipart form body. This writes an end boundary and returns the output writer.
    ///
    /// If you're using something like a `Cursor` as an output writer, you can then get the
    /// contents using:
    /// ```rust,ignore
    /// let bytes = form_data.end()?.into_inner();
    /// ```
    pub fn end(mut self) -> Result<W> {
        write!(&mut self.writer, "--{}--\r\n", self.boundary)?;
        Ok(self.writer)
    }
}

/// Builder for Field values.
#[derive(Debug)]
pub struct FieldBuilder<'s> {
    field: Field<'s, Empty>,
}

impl<'s> FieldBuilder<'s> {
    /// Start building a new field.
    pub fn new(name: impl Into<Cow<'s, str>>) -> Self {
        Self {
            field: Field {
                name: name.into(),
                filename: None,
                content_type: None,
                data: std::io::empty(),
            },
        }
    }

    /// Set the file name for the field.
    pub fn filename(mut self, filename: impl Into<Cow<'s, str>>) -> Self {
        self.field.filename = Some(filename.into());
        self
    }

    /// Set the content type for the field.
    pub fn content_type(mut self, content_type: impl Into<Cow<'s, str>>) -> Self {
        self.field.content_type = Some(content_type.into());
        self
    }

    /// Provide data for and finish the field.
    pub fn build<R>(self, data: R) -> Field<'s, R>
    where
        R: Read,
    {
        Field {
            name: self.field.name,
            filename: self.field.filename,
            content_type: self.field.content_type,
            data,
        }
    }
}

#[derive(Debug)]
pub struct Field<'s, R>
where
    R: Read,
{
    name: Cow<'s, str>,
    filename: Option<Cow<'s, str>>,
    content_type: Option<Cow<'s, str>>,
    data: R,
}

#[cfg(test)]
mod tests {
    use super::FormData;
    use std::io::Cursor;

    #[test]
    fn random_boundary() {
        let buffer = Cursor::new(vec![]);
        let form_data = FormData::new(buffer);

        let boundary = form_data.boundary().as_bytes();
        let (dashes, hex) = boundary.split_at(26);

        for dash in dashes {
            assert_eq!(*dash, b'-');
        }
        for c in hex {
            assert!(matches!(c, b'0'..=b'9' | b'a'..=b'f'))
        }
    }
}

use std::{
    collections::VecDeque,
    error::Error,
    fmt::Display,
    io::{self, Read},
    string::FromUtf8Error,
};

#[derive(Debug)]
pub(crate) enum RequestReaderError {
    Io(io::Error),
    Encoding(FromUtf8Error),
}

impl Display for RequestReaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "encountered an IO error reading the request: {}", e),
            Self::Encoding(e) => write!(f, "could not decode the bytes: {}", e),
        }
    }
}

impl Error for RequestReaderError {}

impl From<io::Error> for RequestReaderError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<FromUtf8Error> for RequestReaderError {
    fn from(value: FromUtf8Error) -> Self {
        Self::Encoding(value)
    }
}

const BUFFERED_READER_BUF_SIZE: usize = 2048;

pub(crate) struct RequestReader<R: Read> {
    reader: R,
    internal: VecDeque<u8>,
}

impl<R: Read> RequestReader<R> {
    pub fn from_reader(r: R) -> Self {
        Self {
            reader: r,
            internal: VecDeque::with_capacity(BUFFERED_READER_BUF_SIZE),
        }
    }

    pub fn read_start_line(&mut self) -> Result<String, RequestReaderError> {
        let bytes = self.read_until_with_chunk_size::<16>("\r\n")?;
        String::from_utf8(bytes)
            .map(|s| s.strip_suffix("\r\n").unwrap().to_owned())
            .map_err(Into::into)
    }

    pub fn read_headers(&mut self) -> Result<Vec<String>, RequestReaderError> {
        let bytes = self.read_until_with_chunk_size::<64>("\r\n\r\n")?;
        let string =
            String::from_utf8(bytes).map(|s| s.strip_suffix("\r\n\r\n").unwrap().to_owned())?;
        Ok(string.split("\r\n").map(ToOwned::to_owned).collect())
    }

    fn read_until_with_chunk_size<const N: usize>(
        &mut self,
        pattern: &str,
    ) -> std::io::Result<Vec<u8>> {
        let pattern_bytes = pattern.as_bytes();
        let mut output = Vec::with_capacity(N);
        let mut buf = [0; N];

        loop {
            let n = self.read(&mut buf)?;
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            if let Some(index) = buf[..n]
                .windows(pattern_bytes.len())
                .position(|w| w == pattern_bytes)
            {
                // Pattern found
                let end_idx = index + pattern_bytes.len();
                output.extend(&buf[..end_idx]);
                if end_idx < n {
                    // Bytes after the pattern should be added to internal buffer for later
                    // reading.
                    self.internal.extend(&buf[end_idx..]);
                }
                return Ok(output);
            }

            // Pattern not found, add all bytes to output
            output.extend(buf);
            buf = [0; N];
        }
    }

    fn internal_read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = buf.len();
        if len == 0 {
            return Ok(0);
        }

        // Read n bytes, where n is the minimum size between the read buffer and the internal
        // buffer (ie: empty the internal buffer)
        let n = len.min(self.internal.len());
        for idx in buf.iter_mut().take(n) {
            // NOTE: This can't be efficient, is there a better way to take n elements and save to
            // a slice?
            *idx = self
                .internal
                .pop_front()
                .expect("Internal buffer empty when expecting data");
        }

        if n < len {
            // Still room in external buffer, read from reader
            let external_free = len - n;
            let mut tmp = [0; BUFFERED_READER_BUF_SIZE];
            let tmp_size = self.reader.read(&mut tmp)?;
            let end_idx = tmp_size.min(external_free);
            buf[n..(n + end_idx)].clone_from_slice(&tmp[..end_idx]);
            // Save remaining to internal buffer
            if tmp_size > end_idx {
                self.internal.extend(&tmp[end_idx..])
            }
            Ok(n + end_idx)
        } else {
            Ok(n)
        }
    }
}

impl<R: Read> Read for RequestReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.internal_read(buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_until_returns_correct_output() {
        let data = "ABCDEF012345\r\nXX".as_bytes();
        let mut req_reader = RequestReader::from_reader(data);
        let out = req_reader
            .read_until_with_chunk_size::<16>("\r\n")
            .expect("error reading until");

        // Expect correct output
        assert_eq!(out, "ABCDEF012345\r\n".as_bytes());

        // 14 first characters read into output, but since chunk is 16 bytes long, expect the
        // remaining 2 bytes to have been read and stored into the internal buffer
        assert_eq!(req_reader.internal, "XX".as_bytes())
    }

    #[test]
    fn read_until_errors_when_pattern_not_found() {
        let data = "ABCD".as_bytes();
        let mut req_reader = RequestReader::from_reader(data);
        let e = req_reader
            .read_until_with_chunk_size::<16>("badpattern")
            .expect_err("expected error when reading until non-existent pattern");

        assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof);
    }
}

use std::{
    error::Error,
    fmt::Display,
    io::{self, Read},
    net::TcpStream,
};

use reader::{RequestReader, RequestReaderError};

pub mod reader;

// TODO: Split Format error into multiple errors / be more descriptive
#[derive(Debug)]
pub enum RequestParsingError {
    Io(io::Error),
    Format,
    UnsupportedVersion(Version),
}

impl Display for RequestParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error parsing request: {}", e),
            Self::Format => write!(f, "unexpected format while parsing request"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported HTTP version: {}", v),
        }
    }
}

impl From<RequestReaderError> for RequestParsingError {
    fn from(value: RequestReaderError) -> Self {
        match value {
            RequestReaderError::Io(e) => Self::Io(e),
            RequestReaderError::Encoding(_) => Self::Format,
        }
    }
}

impl Error for RequestParsingError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Head,
    Post,
    Put,
    Delete,
    Connect,
    Options,
    Trace,
}

impl TryFrom<&str> for Method {
    type Error = RequestParsingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "GET" => Ok(Self::Get),
            "HEAD" => Ok(Self::Head),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "DELETE" => Ok(Self::Delete),
            "CONNECT" => Ok(Self::Connect),
            "OPTIONS" => Ok(Self::Options),
            "TRACE" => Ok(Self::Trace),
            _ => Err(RequestParsingError::Format),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    V0_9,
    V1,
    V1_1,
    V2,
    V3,
}

impl Version {
    pub fn is_supported(&self) -> bool {
        matches!(self, Self::V1 | Self::V1_1)
    }
}

impl TryFrom<&str> for Version {
    type Error = RequestParsingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "HTTP/0.9" => Ok(Self::V0_9),
            "HTTP/1" => Ok(Self::V1),
            "HTTP/1.1" => Ok(Self::V1_1),
            "HTTP/2" => Ok(Self::V2),
            "HTTP/3" => Ok(Self::V3),
            _ => Err(RequestParsingError::Format),
        }
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::V0_9 => write!(f, "HTTP/0.9"),
            Self::V1 => write!(f, "HTTP/1"),
            Self::V1_1 => write!(f, "HTTP/1.1"),
            Self::V2 => write!(f, "HTTP/2"),
            Self::V3 => write!(f, "HTTP/3"),
        }
    }
}

#[derive(Debug)]
pub struct Request {
    method: Method,
    target: String,
    version: Version,
    headers: Vec<String>,
    body: Option<String>,
}

impl Request {
    fn try_from_reader<R: Read>(
        reader: &mut RequestReader<R>,
    ) -> Result<Self, RequestParsingError> {
        let start_line = reader.read_start_line().expect("Error reading start line");

        let mut items = start_line.split(' ');

        let method = items
            .next()
            .ok_or(RequestParsingError::Format)?
            .try_into()?;

        let target = items.next().ok_or(RequestParsingError::Format)?.to_owned();

        let version: Version = items
            .next()
            .ok_or(RequestParsingError::Format)?
            .try_into()?;

        if !version.is_supported() {
            return Err(RequestParsingError::UnsupportedVersion(version));
        }

        if items.count() != 0 {
            // Extra arguments to start line
            return Err(RequestParsingError::Format);
        }

        let headers = reader.read_headers()?;

        // TODO: Body parsing
        Ok(Self {
            method,
            target,
            version,
            headers,
            body: None,
        })
    }
}

impl TryFrom<TcpStream> for Request {
    type Error = RequestParsingError;

    fn try_from(value: TcpStream) -> Result<Self, Self::Error> {
        let mut reader = RequestReader::from_reader(value);
        Self::try_from_reader(&mut reader)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_parses_correctly() {
        let message = "GET / HTTP/1.1\r
Host: 127.0.0.1:8080\r
User-Agent: curl/8.9.1\r
Accept: */*\r
\r
";
        let mut reader = RequestReader::from_reader(message.as_bytes());
        let request = Request::try_from_reader(&mut reader).expect("error parsing request");

        assert_eq!(request.method, Method::Get);
        assert_eq!(request.target, "/");
        assert_eq!(request.version, Version::V1_1);
        assert_eq!(request.headers.len(), 3);
    }

    #[test]
    fn request_errors_when_invalid_method() {
        let message = "GeT / HTTP/1.1\r
Host: 127.0.0.1:8080\r
User-Agent: curl/8.9.1\r
Accept: */*\r
\r
";
        let mut reader = RequestReader::from_reader(message.as_bytes());
        let request_err = Request::try_from_reader(&mut reader)
            .expect_err("expected error while parsing request");

        assert!(matches!(request_err, RequestParsingError::Format));
    }

    #[test]
    fn request_errors_when_invalid_version() {
        let message = "GET / HTTp/1.1\r
Host: 127.0.0.1:8080\r
User-Agent: curl/8.9.1\r
Accept: */*\r
\r
";
        let mut reader = RequestReader::from_reader(message.as_bytes());
        let request_err = Request::try_from_reader(&mut reader)
            .expect_err("expected error while parsing request");

        assert!(matches!(request_err, RequestParsingError::Format));
    }

    #[test]
    fn request_errors_when_invalid_start_line() {
        let message = "GET / HTTP/1.1 extradata\r
Host: 127.0.0.1:8080\r
User-Agent: curl/8.9.1\r
Accept: */*\r
\r
";
        let mut reader = RequestReader::from_reader(message.as_bytes());
        let request_err = Request::try_from_reader(&mut reader)
            .expect_err("expected error while parsing request");

        assert!(matches!(request_err, RequestParsingError::Format));
    }

    #[test]
    fn request_errors_when_invalid_start_line_2() {
        let message = "GET /\r
Host: 127.0.0.1:8080\r
User-Agent: curl/8.9.1\r
Accept: */*\r
\r
";
        let mut reader = RequestReader::from_reader(message.as_bytes());
        let request_err = Request::try_from_reader(&mut reader)
            .expect_err("expected error while parsing request");

        assert!(matches!(request_err, RequestParsingError::Format));
    }
}

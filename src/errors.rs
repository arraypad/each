use failure::Fail;

#[derive(Debug, Fail)]
pub enum EachError {
	#[fail(display = "Incorrect usage: {:?}", message)]
	Usage { message: String },
	#[fail(display = "Data error: {:?}", message)]
	Data { message: String },
	#[fail(display = "IO error: {:?}", inner)]
	Io { inner: std::io::Error },
}

impl From<std::io::Error> for EachError {
	fn from(error: std::io::Error) -> Self {
		EachError::Io { inner: error }
	}
}

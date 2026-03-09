use std::path::PathBuf;

pub(crate) struct Runner {
    source: PathBuf,
    input: PathBuf,
    output: PathBuf,
    max_chars: i64,
}

impl Runner {
    pub(crate) fn new(source: PathBuf, input: PathBuf, output: PathBuf, max_chars: i64) -> Self {
        Self {
            source,
            input,
            output,
            max_chars,
        }
    }

    pub(crate) fn run(&self) {
        cc::Build::new()
            .cpp(true)
            .file(self.source.as_path())
            .out_dir(self.source.parent().unwrap())
            .target("x86_64-w64-mingw32")
            .host("x86_64-w64-mingw32")
            .opt_level(1)
            .compile(self.source.file_stem().and_then(|s| s.to_str()).unwrap());
    }
}

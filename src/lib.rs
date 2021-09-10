use std::{
    collections::BTreeMap,
    fs, io,
    path::{self, Path, PathBuf},
};

use anyhow::bail;
use codespan_reporting::{
    diagnostic::Diagnostic as CodeSpanDiagnostic,
    files::SimpleFiles,
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};

use diagnostic::{Diagnostic, Diagnostics};
use doc_comment::DocComment;
use doc_entry::{ClassDocEntry, DocEntry, FunctionDocEntry, PropertyDocEntry, TypeDocEntry};
use pathdiff::diff_paths;
use serde::Serialize;

use walkdir::{self, WalkDir};

mod cli;
mod diagnostic;
mod doc_comment;
mod doc_entry;
pub mod error;
pub mod realm;
mod serde_util;
pub mod source_file;
mod span;
mod tags;

pub use cli::*;

use error::Error;
use source_file::SourceFile;

/// The class struct that is used in the main output, which owns its members
#[derive(Debug, Serialize)]
struct OutputClass<'a> {
    functions: Vec<FunctionDocEntry<'a>>,
    properties: Vec<PropertyDocEntry<'a>>,
    types: Vec<TypeDocEntry<'a>>,

    #[serde(flatten)]
    class: ClassDocEntry<'a>,
}

type CodespanFilesPaths = (PathBuf, usize);

pub fn generate_docs_from_path(path: &Path) -> anyhow::Result<()> {
    let (codespan_files, files) = find_files(path)?;

    let mut errors: Vec<Error> = Vec::new();
    let mut source_files: Vec<SourceFile> = Vec::new();

    for (file_path, file_id) in files {
        let source = codespan_files.get(file_id).unwrap().source();

        let human_path = match diff_paths(&file_path, path) {
            Some(relative_path) => relative_path,
            None => file_path,
        };

        let human_path = human_path
            .to_string_lossy()
            .to_string()
            .replace(path::MAIN_SEPARATOR, "/");

        match SourceFile::from_str(source, file_id, human_path) {
            Ok(source_file) => source_files.push(source_file),
            Err(error) => errors.push(error),
        }
    }

    let (entries, source_file_errors): (Vec<_>, Vec<_>) = source_files
        .iter()
        .map(SourceFile::parse)
        .partition(Result::is_ok);

    errors.extend(source_file_errors.into_iter().map(Result::unwrap_err));

    let entries: Vec<_> = entries.into_iter().map(Result::unwrap).flatten().collect();

    match into_classes(entries) {
        Ok(classes) => {
            if errors.is_empty() {
                println!("{}", serde_json::to_string_pretty(&classes)?);
            }
        }
        Err(diagnostics) => errors.push(Error::ParseErrors(diagnostics)),
    }

    if !errors.is_empty() {
        let count_errors = errors.len();

        report_errors(errors, &codespan_files);

        if count_errors == 1 {
            bail!("aborting due to diagnostic error");
        } else {
            bail!("aborting due to {} diagnostic errors", count_errors);
        }
    }

    Ok(())
}

fn into_classes<'a>(entries: Vec<DocEntry<'a>>) -> Result<Vec<OutputClass<'a>>, Diagnostics> {
    let mut map: BTreeMap<String, OutputClass<'a>> = BTreeMap::new();

    let (classes, entries): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .partition(|entry| matches!(*entry, DocEntry::Class(_)));

    for entry in classes {
        if let DocEntry::Class(class) = entry {
            let (functions, properties, types) = Default::default();
            map.insert(
                class.name.to_owned(),
                OutputClass {
                    class,
                    functions,
                    properties,
                    types,
                },
            );
        }
    }

    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    let mut emit_diagnostic = |source: &DocComment, within: &str| {
        diagnostics.push(source.diagnostic(format!(
            "This entry's parent class \"{}\" is missing a doc entry",
            within
        )));
    };

    for entry in entries {
        match entry {
            DocEntry::Function(entry) => match map.get_mut(&entry.within) {
                Some(class) => class.functions.push(entry),
                None => emit_diagnostic(entry.source, &entry.within),
            },
            DocEntry::Property(entry) => match map.get_mut(&entry.within) {
                Some(class) => class.properties.push(entry),
                None => emit_diagnostic(entry.source, &entry.within),
            },
            DocEntry::Type(entry) => match map.get_mut(&entry.within) {
                Some(class) => class.types.push(entry),
                None => emit_diagnostic(entry.source, &entry.within),
            },
            _ => unreachable!(),
        };
    }

    if diagnostics.is_empty() {
        Ok(map.into_iter().map(|(_, value)| value).collect())
    } else {
        Err(Diagnostics::from(diagnostics))
    }
}

fn find_files(
    path: &Path,
) -> Result<(SimpleFiles<String, String>, Vec<CodespanFilesPaths>), io::Error> {
    let mut codespan_files = SimpleFiles::new();
    let mut files: Vec<CodespanFilesPaths> = Vec::new();

    let walker = WalkDir::new(path).follow_links(true).into_iter();
    for entry in walker
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".lua"))
    {
        let path = entry.path();
        let contents = fs::read_to_string(path)?;

        let file_id = codespan_files.add(
            // We need the separator to consistently be forward slashes for snapshot
            // consistency across platforms
            path.to_string_lossy().replace(path::MAIN_SEPARATOR, "/"),
            contents,
        );

        files.push((path.to_path_buf(), file_id));
    }

    Ok((codespan_files, files))
}

fn report_errors(errors: Vec<Error>, codespan_files: &SimpleFiles<String, String>) {
    let writer = StandardStream::stderr(ColorChoice::Auto);
    let config = codespan_reporting::term::Config::default();

    for error in errors {
        match error {
            Error::ParseErrors(diagnostics) => {
                for diagnostic in diagnostics.into_iter() {
                    term::emit(
                        &mut writer.lock(),
                        &config,
                        codespan_files,
                        &CodeSpanDiagnostic::from(diagnostic),
                    )
                    .unwrap()
                }
            }
            Error::FullMoonError(error) => eprintln!("{}", error),
        }
    }
}

//! The raw package view: parts, content types, relationships and defects.

use std::collections::BTreeMap;
use std::path::PathBuf;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use pptxboss_core::opc::TargetMode;
use pptxboss_core::{Package as CorePackage, PackageDefects as CorePackageDefects, PackageSeed};

use crate::pptx_err;

/// The raw Open Packaging Conventions view of a deck: every part, content
/// type and relationship as written, defects included.
#[pyclass(frozen, module = "pptxboss")]
pub struct Package {
    seed: PackageSeed,
    path: Option<String>,
}

impl Package {
    pub fn from_seed(seed: PackageSeed, path: Option<String>) -> Self {
        Self { seed, path }
    }

    fn core(&self) -> CorePackage {
        CorePackage::from_seed(self.seed.clone())
    }
}

#[pymethods]
impl Package {
    /// Opens a package from a path, or from bytes with `data=`.
    #[new]
    #[pyo3(signature = (path=None, *, data=None))]
    fn new(py: Python<'_>, path: Option<PathBuf>, data: Option<Vec<u8>>) -> PyResult<Self> {
        let opened = match (path.as_ref(), data) {
            (Some(path), None) => {
                py.allow_threads(|| CorePackage::open(path).map(|package| package.seed()))
            }
            (None, Some(data)) => {
                py.allow_threads(|| CorePackage::from_bytes(data).map(|package| package.seed()))
            }
            _ => {
                return Err(PyValueError::new_err(
                    "pass either a path or data=, not both",
                ))
            }
        };
        Ok(Self {
            seed: opened.map_err(pptx_err)?,
            path: path.map(|path| path.display().to_string()),
        })
    }

    /// Every part in archive order.
    fn parts(&self, py: Python<'_>) -> Vec<Part> {
        let seed = self.seed.clone();
        py.allow_threads(|| {
            let package = CorePackage::from_seed(seed);
            package
                .parts()
                .iter()
                .map(|part| {
                    let entry = package.archive().get(part.entry);
                    Part {
                        name: part.name.clone(),
                        content_type: package.content_type_of(&part.name).map(str::to_string),
                        size: entry.map_or(0, |entry| entry.uncompressed_size),
                        compressed_size: entry.map_or(0, |entry| entry.compressed_size),
                    }
                })
                .collect()
        })
    }

    /// True when a part of that name exists, compared case-insensitively.
    fn has(&self, name: &str) -> bool {
        self.core().has_part(name)
    }

    /// The declared content type of a part, or None.
    fn content_type(&self, name: &str) -> Option<String> {
        self.core().content_type_of(name).map(str::to_string)
    }

    /// The decompressed bytes of a part. A UTF-16 XML part comes back as
    /// UTF-8 unless `raw=True`, which returns the bytes exactly as stored.
    #[pyo3(signature = (name, *, raw=false))]
    fn read<'py>(&self, py: Python<'py>, name: &str, raw: bool) -> PyResult<Bound<'py, PyBytes>> {
        let seed = self.seed.clone();
        let name = name.to_string();
        let bytes = py
            .allow_threads(|| {
                let package = CorePackage::from_seed(seed);
                if !raw {
                    return package.read_part(&name).map(|data| data.as_ref().clone());
                }
                let mut out = Vec::new();
                package.read_part_bytes(&name, &mut out)?;
                Ok(out)
            })
            .map_err(pptx_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// The relationships of a part, or of the package itself for `/`.
    #[pyo3(signature = (source="/"))]
    fn rels(&self, py: Python<'_>, source: &str) -> PyResult<Vec<Relationship>> {
        let seed = self.seed.clone();
        let source = source.to_string();
        py.allow_threads(|| {
            let rels = CorePackage::from_seed(seed).rels(&source)?;
            Ok::<_, pptxboss_core::Error>(
                rels.iter()
                    .map(|rel| Relationship {
                        id: rel.id.clone(),
                        rel_type: rel.rel_type.clone(),
                        target: rel.target.clone(),
                        external: rel.mode == TargetMode::External,
                    })
                    .collect(),
            )
        })
        .map_err(pptx_err)
    }

    /// The part name relationship `rel_id` of `source` points at, when internal.
    fn resolve(&self, py: Python<'_>, source: &str, rel_id: &str) -> PyResult<Option<String>> {
        let seed = self.seed.clone();
        let source = source.to_string();
        let rel_id = rel_id.to_string();
        py.allow_threads(|| CorePackage::from_seed(seed).resolve(&source, &rel_id))
            .map_err(pptx_err)
    }

    /// The `[Content_Types].xml` stream: defaults by extension, overrides by part name.
    fn content_types(&self, py: Python<'_>) -> ContentTypes {
        let seed = self.seed.clone();
        py.allow_threads(|| {
            let package = CorePackage::from_seed(seed);
            let types = package.content_types();
            ContentTypes {
                defaults: types.defaults().iter().cloned().collect(),
                overrides: types.overrides().iter().cloned().collect(),
            }
        })
    }

    /// What the package layer found wrong while indexing.
    #[getter]
    fn defects(&self, py: Python<'_>) -> PackageDefects {
        let seed = self.seed.clone();
        py.allow_threads(|| PackageDefects::from_core(CorePackage::from_seed(seed).defects()))
    }

    fn __repr__(&self) -> String {
        let parts = self.core().parts().len();
        match &self.path {
            Some(path) => format!("Package(path={path:?}, parts={parts})"),
            None => format!("Package(parts={parts})"),
        }
    }
}

/// One part of a package.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct Part {
    #[pyo3(get)]
    name: String,
    #[pyo3(get)]
    content_type: Option<String>,
    /// Decompressed size in bytes.
    #[pyo3(get)]
    size: u64,
    #[pyo3(get)]
    compressed_size: u64,
}

#[pymethods]
impl Part {
    fn __repr__(&self) -> String {
        format!("Part({:?}, {} bytes)", self.name, self.size)
    }
}

/// One relationship of a part or of the package.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct Relationship {
    #[pyo3(get)]
    id: String,
    /// The relationship type URI.
    #[pyo3(get, name = "type")]
    rel_type: String,
    /// The target as written; `Package.resolve` gives the part name.
    #[pyo3(get)]
    target: String,
    #[pyo3(get)]
    external: bool,
}

#[pymethods]
impl Relationship {
    fn __repr__(&self) -> String {
        format!("Relationship({} -> {:?})", self.id, self.target)
    }
}

/// The content types stream as two dicts.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct ContentTypes {
    /// Extension to media type.
    #[pyo3(get)]
    defaults: BTreeMap<String, String>,
    /// Part name to media type.
    #[pyo3(get)]
    overrides: BTreeMap<String, String>,
}

#[pymethods]
impl ContentTypes {
    fn __repr__(&self) -> String {
        format!(
            "ContentTypes(defaults={}, overrides={})",
            self.defaults.len(),
            self.overrides.len()
        )
    }
}

/// What the package layer found wrong: missing or broken content types,
/// bad part names, name collisions, directory entries and incomplete pieces.
#[pyclass(frozen, module = "pptxboss")]
#[derive(Clone)]
pub struct PackageDefects {
    #[pyo3(get)]
    content_types_missing: bool,
    #[pyo3(get)]
    content_types_case: Option<String>,
    #[pyo3(get)]
    content_types_error: Option<String>,
    #[pyo3(get)]
    content_types_unreadable: Option<String>,
    /// `(part name, why it is invalid)` pairs.
    #[pyo3(get)]
    invalid_names: Vec<(String, String)>,
    /// Pairs of part names that are equivalent under OPC name comparison.
    #[pyo3(get)]
    collisions: Vec<(String, String)>,
    /// Pairs where one part name is derivable from another.
    #[pyo3(get)]
    derivable: Vec<(String, String)>,
    #[pyo3(get)]
    directories: Vec<String>,
    #[pyo3(get)]
    incomplete_pieces: Vec<String>,
}

impl PackageDefects {
    fn from_core(defects: &CorePackageDefects) -> Self {
        Self {
            content_types_missing: defects.content_types_missing,
            content_types_case: defects.content_types_case.clone(),
            content_types_error: defects
                .content_types_error
                .as_ref()
                .map(|err| err.to_string()),
            content_types_unreadable: defects.content_types_unreadable.clone(),
            invalid_names: defects
                .invalid_names
                .iter()
                .map(|(name, err)| (name.clone(), err.to_string()))
                .collect(),
            collisions: defects.collisions.clone(),
            derivable: defects.derivable.clone(),
            directories: defects.directories.clone(),
            incomplete_pieces: defects.incomplete_pieces.clone(),
        }
    }
}

#[pymethods]
impl PackageDefects {
    fn __repr__(&self) -> String {
        format!(
            "PackageDefects(content_types_missing={}, invalid_names={}, collisions={})",
            self.content_types_missing,
            self.invalid_names.len(),
            self.collisions.len()
        )
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Package>()?;
    m.add_class::<Part>()?;
    m.add_class::<Relationship>()?;
    m.add_class::<ContentTypes>()?;
    m.add_class::<PackageDefects>()?;
    Ok(())
}

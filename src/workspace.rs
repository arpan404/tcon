use std::path::Path;
use std::path::PathBuf;

/// Represent a discovered workspace, which is a directory containing `./tcon/`.
/// .tcon is the directory where tcon files should be stored.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// The path to the workspace directory: parent dir of `.tcon/`
    pub root: PathBuf,

    /// The absolute path to the `.tcon/` directory where tcon files are stored.
    pub tcon_dir: PathBuf,
}

impl Workspace {
    /// Discover workspace by:
    /// - using  the provided root, or current working directory if root is None.
    /// - checking the presence of `.tcon/` directory
    pub fn discover(root: Option<&str>) -> Result<Self, String> {
        let root = match root {
            Some(r) => PathBuf::from(r),
            None => std::env::current_dir().map_err(|e| format!("failed reading cwd: {e}"))?,
        };

        let tcon_dir = root.join(".tcon");
        if !tcon_dir.is_dir() {
            return Err(format!(
                "Missing .tcon directory at {}",
                tcon_dir.display()
            ));
        }
        Ok(Self { root, tcon_dir })
    }

    /// Find all the entry files under `.tcon/` with the `.tcon` extension
    pub fn find_tcon_entries(&self) -> Result<Vec<PathBuf>, String> {
        let mut out = Vec::new();
        for f in std::fs::read_dir(&self.tcon_dir)
            .map_err(|e| format!("failed to list .tcon directory: {e}"))?
        {
            let f = f.map_err(|e| format!("failed to read dir entry: {e}"))?;
            let p = f.path();

            // Include only the files
            if p.is_file() {
                if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
                    // Track the only file with `.tcon` extension
                    if name.ends_with(".tcon") {
                        out.push(p);
                    }
                }
            }
        }
        // sort so the output is determinstic across OS/filesystem order.
        out.sort();
        Ok(out)
    }
    /// Resolve an entry argument:
    /// - if absoulte: accept it
    /// - otherwise treat it as a relative tot `.tcon/`
    pub fn resolve_entry(&self, entry: &str) -> Result<PathBuf, String> {
        let p = Path::new(entry);

        if p.is_absolute() {
            return Ok(p.to_path_buf());
        }

        let candidate = self.tcon_dir.join(entry);
        if candidate.exists() {
            Ok(candidate)
        } else {
            Err(format!(
                "File not found: {}",
                candidate.display()
            ))
        }
    }
}

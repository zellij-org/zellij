//! Deterministic bundling of the web client frontend assets.
use crate::flags;
use anyhow::{anyhow, Context};
use sha2::{Digest, Sha384};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use xshell::Shell;

const MODULE_ORDER: &[&str] = &[
    "utils",
    "connection",
    "auth",
    "keyboard",
    "links",
    "terminal",
    "ime-bypass",
    "soft-keyboard",
    "key-handler",
    "mouse",
    "pinch",
    "mobile-pan",
    "touch",
    "input",
    "mobile-ui",
    "websockets",
    "index",
];

const HASHED_ASSETS: &[&str] = &[
    "app.js",
    "xterm.js",
    "addon-fit.js",
    "addon-clipboard.js",
    "addon-web-links.js",
    "addon-webgl.js",
    "modals.js",
    "xterm.css",
    "style.css",
];

const BUNDLE_FILE: &str = "app.js";
const INTEGRITY_FILE: &str = "integrity.json";
const INDEX_FILE: &str = "index.html";

const DECLARATION_PREFIXES: &[&str] = &[
    "async function ",
    "function ",
    "const ",
    "let ",
    "var ",
    "class ",
];

pub fn assets(_sh: &Shell, flags: flags::Assets) -> anyhow::Result<()> {
    let msg = if flags.check {
        ">> Checking bundled web client assets"
    } else {
        ">> Bundling web client assets"
    };
    crate::status(msg);
    println!("{}", msg);

    let assets_dir = web_assets_dir();
    let generated = generate(&assets_dir)?;

    if flags.check {
        for (name, contents) in &generated {
            let path = assets_dir.join(name);
            let on_disk = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read '{}'", path.display()))?;
            if &on_disk != contents {
                return Err(anyhow!(
                    "'{}' is out of date, run `cargo xtask assets`",
                    path.display()
                ));
            }
        }
        return Ok(());
    }

    for (name, contents) in &generated {
        let path = assets_dir.join(name);
        std::fs::write(&path, contents)
            .with_context(|| format!("failed to write '{}'", path.display()))?;
    }
    Ok(())
}

fn web_assets_dir() -> PathBuf {
    crate::project_root().join("zellij-client").join("assets")
}

fn generate(assets_dir: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let bundle = build_bundle(assets_dir)?;

    let mut digests: BTreeMap<String, String> = BTreeMap::new();
    for asset in HASHED_ASSETS {
        let bytes = if *asset == BUNDLE_FILE {
            bundle.as_bytes().to_vec()
        } else {
            let path = assets_dir.join(asset);
            std::fs::read(&path).with_context(|| format!("failed to read '{}'", path.display()))?
        };
        digests.insert((*asset).to_string(), subresource_integrity(&bytes));
    }

    let integrity = format!("{}\n", serde_json::to_string_pretty(&digests)?);

    let index_path = assets_dir.join(INDEX_FILE);
    let index_source = std::fs::read_to_string(&index_path)
        .with_context(|| format!("failed to read '{}'", index_path.display()))?;
    let index = rewrite_integrity_attributes(&index_source, &digests)?;

    Ok(vec![
        (BUNDLE_FILE.to_string(), bundle),
        (INTEGRITY_FILE.to_string(), integrity),
        (INDEX_FILE.to_string(), index),
    ])
}

fn build_bundle(assets_dir: &Path) -> anyhow::Result<String> {
    let mut bundle = String::new();
    let mut declarations: BTreeMap<String, String> = BTreeMap::new();

    for module in MODULE_ORDER {
        let path = assets_dir.join(format!("{}.js", module));
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read '{}'", path.display()))?;
        let chunk = flatten_module(module, &source)?;
        for name in top_level_declarations(&chunk) {
            if let Some(previous) = declarations.insert(name.clone(), (*module).to_string()) {
                return Err(anyhow!(
                    "duplicate top-level identifier '{}' declared in both '{}.js' and '{}.js'",
                    name,
                    previous,
                    module
                ));
            }
        }
        bundle.push_str(&chunk);
        if !bundle.ends_with('\n') {
            bundle.push('\n');
        }
    }

    Ok(bundle)
}

fn flatten_module(module: &str, source: &str) -> anyhow::Result<String> {
    let mut out = String::new();
    let mut lines = source.lines().enumerate().peekable();

    while let Some((index, line)) = lines.next() {
        let location = || format!("{}.js:{}", module, index + 1);

        if line.contains("import(") {
            return Err(anyhow!("dynamic import is not supported at {}", location()));
        }

        if let Some(rest) = line.strip_prefix("import ") {
            if is_terminated_import(rest) {
                validate_module_specifier(rest, &location())?;
                continue;
            }
            let mut terminated = false;
            for (_, continuation) in lines.by_ref() {
                let trimmed = continuation.trim_start();
                if trimmed.starts_with("} from ") && trimmed.ends_with(';') {
                    validate_module_specifier(trimmed, &location())?;
                    terminated = true;
                    break;
                }
                if !is_import_binding_line(trimmed) {
                    return Err(anyhow!("unrecognised import syntax at {}", location()));
                }
            }
            if !terminated {
                return Err(anyhow!("unterminated import statement at {}", location()));
            }
            continue;
        }

        if line.starts_with("export ") || line.starts_with("export{") {
            if line.starts_with("export {") {
                if line.contains(" from ") {
                    validate_module_specifier(line, &location())?;
                    continue;
                }
                return Err(anyhow!("unrecognised export syntax at {}", location()));
            }
            if line.starts_with("export default") || line.starts_with("export *") {
                return Err(anyhow!("unsupported export form at {}", location()));
            }
            let stripped = &line["export ".len()..];
            if !DECLARATION_PREFIXES
                .iter()
                .any(|prefix| stripped.starts_with(prefix))
            {
                return Err(anyhow!("unrecognised export syntax at {}", location()));
            }
            out.push_str(stripped);
            out.push('\n');
            continue;
        }

        out.push_str(line);
        out.push('\n');
    }

    Ok(out)
}

fn is_terminated_import(rest: &str) -> bool {
    rest.ends_with(';') && rest.contains(" from ")
}

fn is_import_binding_line(trimmed: &str) -> bool {
    if trimmed.is_empty() || trimmed == "{" {
        return true;
    }
    trimmed
        .trim_end_matches(',')
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == ' ')
}

fn validate_module_specifier(line: &str, location: &str) -> anyhow::Result<()> {
    let specifier = line
        .rsplit_once(" from ")
        .map(|(_, specifier)| specifier)
        .ok_or_else(|| anyhow!("missing module specifier at {}", location))?
        .trim()
        .trim_end_matches(';')
        .trim_matches(|c| c == '"' || c == '\'');

    let name = specifier
        .strip_prefix("./")
        .and_then(|name| name.strip_suffix(".js"))
        .ok_or_else(|| {
            anyhow!(
                "only relative sibling module specifiers are supported, found '{}' at {}",
                specifier,
                location
            )
        })?;

    if !MODULE_ORDER.contains(&name) {
        return Err(anyhow!(
            "module '{}' referenced at {} is not part of the bundle",
            name,
            location
        ));
    }
    Ok(())
}

fn top_level_declarations(chunk: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in chunk.lines() {
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        for prefix in DECLARATION_PREFIXES {
            let Some(rest) = line.strip_prefix(prefix) else {
                continue;
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$')
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
            break;
        }
    }
    names
}

fn subresource_integrity(bytes: &[u8]) -> String {
    let mut hasher = Sha384::new();
    hasher.update(bytes);
    format!("sha384-{}", base64_encode(&hasher.finalize()))
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn rewrite_integrity_attributes(
    source: &str,
    digests: &BTreeMap<String, String>,
) -> anyhow::Result<String> {
    let mut out = String::with_capacity(source.len());
    let trailing_newline = source.ends_with('\n');

    for line in source.lines() {
        let mut rewritten = line.to_string();
        if let Some(asset) = referenced_asset(line, digests) {
            let digest = &digests[&asset];
            rewritten = replace_integrity_value(&rewritten, digest).ok_or_else(|| {
                anyhow!(
                    "tag referencing '{}' is missing an integrity attribute",
                    asset
                )
            })?;
        }
        out.push_str(&rewritten);
        out.push('\n');
    }

    if !trailing_newline {
        out.pop();
    }
    Ok(out)
}

fn referenced_asset(line: &str, digests: &BTreeMap<String, String>) -> Option<String> {
    for asset in digests.keys() {
        for attribute in ["src=\"assets/", "href=\"assets/"] {
            let needle = format!("{}{}\"", attribute, asset);
            if line.contains(&needle) {
                return Some(asset.clone());
            }
        }
    }
    None
}

fn replace_integrity_value(line: &str, digest: &str) -> Option<String> {
    let start = line.find("integrity=\"")? + "integrity=\"".len();
    let end = start + line[start..].find('"')?;
    let mut out = String::with_capacity(line.len() + digest.len());
    out.push_str(&line[..start]);
    out.push_str(digest);
    out.push_str(&line[end..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_statements_are_dropped() {
        let source = "import { a } from \"./utils.js\";\nexport function b() {}\n";
        let flattened = flatten_module("terminal", source).expect("flatten");
        assert_eq!(flattened, "function b() {}\n");
    }

    #[test]
    fn multiline_imports_are_dropped() {
        let source = "import {\n    a,\n    b,\n} from \"./utils.js\";\nconst c = 1;\n";
        let flattened = flatten_module("terminal", source).expect("flatten");
        assert_eq!(flattened, "const c = 1;\n");
    }

    #[test]
    fn re_exports_are_dropped() {
        let source = "export { setSoftKeyboard } from \"./soft-keyboard.js\";\n";
        let flattened = flatten_module("input", source).expect("flatten");
        assert_eq!(flattened, "");
    }

    #[test]
    fn unsupported_export_forms_are_rejected() {
        assert!(flatten_module("utils", "export default function a() {}\n").is_err());
        assert!(flatten_module("utils", "export * from \"./links.js\";\n").is_err());
        assert!(flatten_module("utils", "export unexpected;\n").is_err());
    }

    #[test]
    fn unknown_module_specifiers_are_rejected() {
        assert!(flatten_module("utils", "import { a } from \"nowhere\";\n").is_err());
        assert!(flatten_module("utils", "import { a } from \"./nowhere.js\";\n").is_err());
    }

    #[test]
    fn dynamic_imports_are_rejected() {
        assert!(flatten_module("utils", "const m = await import(\"./utils.js\");\n").is_err());
    }

    #[test]
    fn duplicate_top_level_identifiers_are_detected() {
        let dir = std::env::temp_dir().join("zellij-xtask-assets-duplicate-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        for module in MODULE_ORDER {
            let contents = if *module == "utils" || *module == "connection" {
                "function collide() {}\n"
            } else {
                ""
            };
            std::fs::write(dir.join(format!("{}.js", module)), contents).expect("write module");
        }

        let error = build_bundle(&dir).expect_err("expected duplicate detection to fail");
        assert!(
            error.to_string().contains("duplicate top-level identifier"),
            "unexpected error: {}",
            error
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn top_level_declarations_ignores_nested_scopes() {
        let chunk = "function outer() {\n    const inner = 1;\n}\nconst top = 2;\n";
        assert_eq!(top_level_declarations(chunk), vec!["outer", "top"]);
    }

    #[test]
    fn integrity_attributes_are_rewritten() {
        let mut digests = BTreeMap::new();
        digests.insert("app.js".to_string(), "sha384-abc".to_string());
        let source = "<script type=\"module\" src=\"assets/app.js\" integrity=\"\"></script>\n";
        let rewritten = rewrite_integrity_attributes(source, &digests).expect("rewrite");
        assert_eq!(
            rewritten,
            "<script type=\"module\" src=\"assets/app.js\" integrity=\"sha384-abc\"></script>\n"
        );
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
    }
}

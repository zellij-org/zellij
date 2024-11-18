// The following license refers to code in this file and this file only.
// We chose to vendor this dependency rather than depend on it through crates.io in order to facilitate
// packaging. This license was copied verbatim from: https://docs.rs/crate/common-path/1.0.0/source/LICENSE-MIT
//
// MIT License
//
// Copyright 2018 Paul Woolcock <paul@woolcock.us>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use std::path::{Path, PathBuf};

/// Find the common prefix, if any, between any number of paths
///
/// # Example
///
/// ```rust
/// use std::path::{PathBuf, Path};
/// use zellij_utils::common_path::common_path_all;
///
/// # fn main() {
/// let baz = Path::new("/foo/bar/baz");
/// let quux = Path::new("/foo/bar/quux");
/// let foo = Path::new("/foo/bar/foo");
/// let prefix = common_path_all(vec![baz, quux, foo]).unwrap();
/// assert_eq!(prefix, Path::new("/foo/bar").to_path_buf());
/// # }
/// ```
pub fn common_path_all<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Option<PathBuf> {
    let mut path_iter = paths.into_iter();
    let mut result = path_iter.next()?.to_path_buf();
    for path in path_iter {
        if let Some(r) = common_path(&result, &path) {
            result = r;
        } else {
            return None;
        }
    }
    Some(result.to_path_buf())
}

/// Find the common prefix, if any, between 2 paths
///
/// # Example
///
/// ```rust
/// use std::path::{PathBuf, Path};
/// use zellij_utils::common_path::common_path;
///
/// # fn main() {
/// let baz = Path::new("/foo/bar/baz");
/// let quux = Path::new("/foo/bar/quux");
/// let prefix = common_path(baz, quux).unwrap();
/// assert_eq!(prefix, Path::new("/foo/bar").to_path_buf());
/// # }
/// ```
pub fn common_path<P, Q>(one: P, two: Q) -> Option<PathBuf>
where
    P: AsRef<Path>,
    Q: AsRef<Path>,
{
    let one = one.as_ref();
    let two = two.as_ref();
    let one = one.components();
    let two = two.components();
    let mut final_path = PathBuf::new();
    let mut found = false;
    let paths = one.zip(two);
    for (l, r) in paths {
        if l == r {
            final_path.push(l.as_os_str());
            found = true;
        } else {
            break;
        }
    }
    if found {
        Some(final_path)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_all_paths() {
        let one = Path::new("/foo/bar/baz/one.txt");
        let two = Path::new("/foo/bar/quux/quuux/two.txt");
        let three = Path::new("/foo/bar/baz/foo/bar");
        let result = Path::new("/foo/bar");
        let path_permutations = vec![
            vec![one, two, three],
            vec![one, three, two],
            vec![two, one, three],
            vec![two, three, one],
            vec![three, one, two],
            vec![three, two, one],
        ];
        for all in path_permutations {
            assert_eq!(common_path_all(all).unwrap(), result.to_path_buf())
        }
    }

    #[test]
    fn compare_paths() {
        let one = Path::new("/foo/bar/baz/one.txt");
        let two = Path::new("/foo/bar/quux/quuux/two.txt");
        let result = Path::new("/foo/bar");
        assert_eq!(common_path(&one, &two).unwrap(), result.to_path_buf())
    }

    #[test]
    fn no_common_path() {
        let one = Path::new("/foo/bar");
        let two = Path::new("./baz/quux");
        assert!(common_path(&one, &two).is_none());
    }
}

//! Moteur : analyse parallèle, protections, résidus connus, doublons, corbeille.
use rayon::prelude::*;
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering::Relaxed},
};

pub static SEEN: AtomicU64 = AtomicU64::new(0);

pub struct Node {
    pub name: String,
    pub size: u64,
    pub files: u64,
    pub is_dir: bool,
    pub children: Vec<Node>,
}

pub struct Root {
    pub path: PathBuf,
    pub node: Node,
}

// --- Analyse -----------------------------------------------------------------

enum Kind {
    Skip,
    Dir,
    File(u64),
}

fn kind(md: &fs::Metadata) -> Kind {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let a = md.file_attributes();
        if a & 0x400 != 0 && a & 0x10 != 0 {
            return Kind::Skip; // jonction / lien symbolique de dossier : évite boucles et doubles comptes
        }
        if a & 0x10 != 0 {
            return Kind::Dir;
        }
        // fichiers « dans le cloud » (OneDrive…) : 0 octet réellement occupé
        let offline = a & (0x1000 | 0x40000 | 0x400000) != 0;
        Kind::File(if offline { 0 } else { md.len() })
    }
    #[cfg(not(windows))]
    {
        let t = md.file_type();
        if t.is_symlink() {
            Kind::Skip
        } else if t.is_dir() {
            Kind::Dir
        } else {
            Kind::File(md.len())
        }
    }
}

/// shortcut: parcours parallèle via read_dir (taille logique) ; passer à la lecture
/// directe de la MFT (droits admin) si un volume de plusieurs To reste trop lent.
pub fn scan(path: &Path, name: String) -> Node {
    let mut node = Node { name, size: 0, files: 0, is_dir: true, children: vec![] };
    let Ok(rd) = fs::read_dir(path) else { return node }; // accès refusé : ignoré
    let mut dirs = vec![];
    for e in rd.flatten() {
        let Ok(md) = e.metadata() else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        match kind(&md) {
            Kind::Skip => {}
            Kind::Dir => dirs.push((name, e.path())),
            Kind::File(size) => {
                SEEN.fetch_add(1, Relaxed);
                node.size += size;
                node.files += 1;
                node.children.push(Node { name, size, files: 1, is_dir: false, children: vec![] });
            }
        }
    }
    SEEN.fetch_add(dirs.len() as u64, Relaxed);
    let subs: Vec<Node> = dirs.into_par_iter().map(|(n, p)| scan(&p, n)).collect();
    for s in subs {
        node.size += s.size;
        node.files += s.files;
        node.children.push(s);
    }
    node.children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
    node
}

pub fn normalize(p: &str) -> PathBuf {
    let mut s = p.trim().trim_matches('"').to_string();
    if s.len() == 2 && s.ends_with(':') {
        s.push('\\');
    } else {
        while s.len() > 3 && (s.ends_with('\\') || s.ends_with('/')) {
            s.pop();
        }
    }
    PathBuf::from(s)
}

pub fn display_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string_lossy().into_owned())
}

pub fn find<'a>(r: &'a Root, p: &Path) -> Option<&'a Node> {
    let mut n = &r.node;
    for c in p.strip_prefix(&r.path).ok()?.components() {
        let s = c.as_os_str().to_str()?;
        n = n.children.iter().find(|x| x.name == s)?;
    }
    Some(n)
}

/// Retire un élément de l'arbre et corrige les tailles des ancêtres.
pub fn remove(r: &mut Root, p: &Path) -> Option<(u64, u64)> {
    fn rec(n: &mut Node, comps: &[String]) -> Option<(u64, u64)> {
        let (first, rest) = comps.split_first()?;
        let i = n.children.iter().position(|c| &c.name == first)?;
        let (s, f) = if rest.is_empty() {
            let c = n.children.remove(i);
            (c.size, c.files)
        } else {
            rec(&mut n.children[i], rest)?
        };
        n.size -= s;
        n.files -= f;
        n.children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
        Some((s, f))
    }
    let comps: Vec<String> = p.strip_prefix(&r.path).ok()?.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    rec(&mut r.node, &comps)
}

// --- Protections ---------------------------------------------------------------

const WIN_INSIDE: &[&str] = &["system volume information", "$recycle.bin", "recovery", "boot", "efi", "$winreagent"];
const WIN_TOP: &[&str] = &["bootmgr", "pagefile.sys", "hiberfil.sys", "swapfile.sys", "users", "program files", "program files (x86)", "programdata"];
const POSIX_WHOLE: &[&str] = &["bin", "sbin", "usr", "etc", "boot", "lib", "lib64", "system", "dev", "proc", "sys"];

/// Chemin Windows normalisé (minuscules, séparateurs doublés/`.`/`..`/préfixe `\\?\` résolus) :
/// renvoie la forme complète et la liste des composants hors lecteur.
fn parts_of(p: &Path) -> (String, Vec<String>) {
    let mut s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    if let Some(r) = s.strip_prefix(r"\\?\") {
        s = r.to_string();
    }
    let (drive, rest) = if s.as_bytes().get(1) == Some(&b':') && s.is_char_boundary(2) { s.split_at(2) } else { ("", &s[..]) };
    let mut parts: Vec<String> = vec![];
    for c in rest.split('\\') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            x => parts.push(x.to_string()),
        }
    }
    (format!("{drive}\\{}", parts.join("\\")), parts)
}

/// Zone système : on n'y analyse pas les doublons et on n'y supprime rien.
pub fn is_system(p: &Path) -> bool {
    if cfg!(windows) {
        let (s, parts) = parts_of(p);
        let win = parts_of(Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()))).0;
        s == win || s.starts_with(&format!("{win}\\")) || parts.first().is_some_and(|f| WIN_INSIDE.contains(&f.as_str()))
    } else {
        p.components().nth(1).and_then(|c| c.as_os_str().to_str()).is_some_and(|f| POSIX_WHOLE.contains(&f))
    }
}

/// Jamais supprimable : zone système, racine de lecteur, dossiers de premier niveau vitaux.
pub fn is_protected(p: &Path) -> bool {
    if is_system(p) {
        return true;
    }
    if cfg!(windows) {
        let (_, parts) = parts_of(p);
        parts.is_empty() || (parts.len() == 1 && WIN_TOP.contains(&parts[0].as_str()))
    } else {
        p.parent().is_none()
    }
}

// --- Éléments, explications, résidus ---------------------------------------------

#[derive(Serialize)]
pub struct Item {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub files: u64,
    pub is_dir: bool,
    pub protected: bool,
    pub explain: &'static str,
}

pub fn item(n: &Node, p: &Path, name: String) -> Item {
    Item {
        name,
        path: p.to_string_lossy().into_owned(),
        size: n.size,
        files: n.files,
        is_dir: n.is_dir,
        protected: is_protected(p),
        explain: explain(&n.name.to_lowercase()),
    }
}

// (nom en minuscules, explication, profil)
const RULES: &[(&str, &str, &str)] = &[
    ("node_modules", "Dépendances npm : se réinstallent avec npm install", "dev"),
    ("__pycache__", "Bytecode Python : se régénère", "dev"),
    (".cache", "Cache d'outils : se régénère", "dev"),
    (".gradle", "Cache Gradle : se retélécharge", "dev"),
    (".next", "Cache de build Next.js : se régénère", "dev"),
    (".nuxt", "Cache de build Nuxt : se régénère", "dev"),
    (".parcel-cache", "Cache Parcel : se régénère", "dev"),
    (".turbo", "Cache Turborepo : se régénère", "dev"),
    (".pytest_cache", "Cache pytest : se régénère", "dev"),
    (".mypy_cache", "Cache mypy : se régénère", "dev"),
    ("media cache files", "Cache média Adobe : se régénère", "creative"),
    ("media cache", "Cache média Adobe : se régénère", "creative"),
    ("peak files", "Fichiers de forme d'onde Adobe : se régénèrent", "creative"),
    ("render files", "Rendus Final Cut : se régénèrent", "creative"),
    ("shadercache", "Cache de shaders de jeu : se régénère", "gamer"),
    ("dxcache", "Cache de shaders DirectX : se régénère", "gamer"),
    ("glcache", "Cache de shaders OpenGL : se régénère", "gamer"),
    ("gpucache", "Cache GPU d'application : se régénère", "general"),
    ("code cache", "Cache de code d'application : se régénère", "general"),
];

fn why(name: &str, siblings: &[Node]) -> Option<(&'static str, &'static str)> {
    if let Some(r) = RULES.iter().find(|r| r.0 == name) {
        return Some((r.1, r.2));
    }
    let has = |pred: fn(&str) -> bool| siblings.iter().any(|s| pred(&s.name.to_lowercase()));
    match name {
        "target" if has(|s| s == "cargo.toml" || s == "pom.xml") => Some(("Sortie de compilation Rust/Maven : se régénère au build", "dev")),
        "bin" | "obj" if has(|s| s.ends_with("proj")) => Some(("Sortie de compilation .NET : se régénère au build", "dev")),
        _ => None,
    }
}

pub fn explain(name: &str) -> &'static str {
    if let Some(r) = RULES.iter().find(|r| r.0 == name) {
        return r.1;
    }
    match name {
        "hiberfil.sys" => "Copie de la RAM pour l'hibernation. Se libère avec « powercfg /h off », pas à la main.",
        "pagefile.sys" => "Fichier d'échange (mémoire virtuelle), géré par Windows.",
        "swapfile.sys" => "Fichier d'échange des applications Store, géré par Windows.",
        "temp" | "tmp" => "Fichiers temporaires : généralement sûrs à supprimer (ceux en cours d'usage sont refusés).",
        "appdata" => "Réglages et données des applications de l'utilisateur : ne pas supprimer en bloc.",
        "windows.old" => "Ancienne installation de Windows : supprimable via le Nettoyage de disque.",
        "winsxs" => "Magasin de composants Windows : ne jamais toucher.",
        "$recycle.bin" => "Corbeille du lecteur.",
        "system volume information" => "Points de restauration et index système : protégé.",
        "programdata" => "Données partagées des applications installées.",
        "users" => "Profils utilisateurs.",
        "windows" => "Système Windows : protégé.",
        "program files" | "program files (x86)" => "Applications installées : désinstallez-les plutôt que de supprimer le dossier.",
        _ => "",
    }
}

#[derive(Serialize)]
pub struct Row {
    pub path: String,
    pub size: u64,
    pub detail: String,
    pub tag: &'static str,
}

#[derive(Serialize)]
pub struct Rows {
    pub rows: Vec<Row>,
    pub skipped: usize,
}

pub fn find_caches(start: &Node, start_path: &Path) -> Rows {
    let (mut rows, mut skipped) = (vec![], 0);
    let mut stack = vec![(start, start_path.to_path_buf())];
    while let Some((d, dp)) = stack.pop() {
        for c in d.children.iter().filter(|c| c.is_dir) {
            let p = dp.join(&c.name);
            match why(&c.name.to_lowercase(), &d.children) {
                Some(_) if c.size == 0 => {}
                Some(_) if is_protected(&p) => skipped += 1,
                Some((w, tag)) => rows.push(Row { path: p.to_string_lossy().into_owned(), size: c.size, detail: w.into(), tag }),
                None => stack.push((c, p)),
            }
        }
    }
    rows.sort_by(|a, b| b.size.cmp(&a.size));
    Rows { rows, skipped }
}

// --- Doublons ------------------------------------------------------------------

pub fn collect_files(n: &Node, p: &Path, min: u64, out: &mut Vec<(u64, PathBuf)>) {
    for c in &n.children {
        let cp = p.join(&c.name);
        if c.is_dir {
            if !is_system(&cp) {
                collect_files(c, &cp, min, out);
            }
        } else if c.size >= min {
            out.push((c.size, cp));
        }
    }
}

fn hash(p: &Path, limit: Option<usize>) -> Option<u64> {
    let mut f = fs::File::open(p).ok()?;
    let mut h = xxhash_rust::xxh3::Xxh3::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut left = limit.unwrap_or(usize::MAX);
    while left > 0 {
        let want = buf.len().min(left);
        let n = f.read(&mut buf[..want]).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        left -= n;
    }
    Some(h.digest())
}

fn same_hash(items: Vec<(u64, PathBuf)>, limit: Option<usize>) -> Vec<Vec<(u64, PathBuf)>> {
    let hashed: Vec<((u64, u64), PathBuf)> = items.into_par_iter().filter_map(|(s, p)| hash(&p, limit).map(|h| ((s, h), p))).collect();
    let mut groups: HashMap<(u64, u64), Vec<(u64, PathBuf)>> = HashMap::new();
    for (k, p) in hashed {
        groups.entry(k).or_default().push((k.0, p));
    }
    groups.into_values().filter(|g| g.len() > 1).collect()
}

/// Taille → hash XXH3 des 64 premiers Ko → hash XXH3 complet. Renvoie les lignes à supprimer
/// (tous les exemplaires sauf le premier de chaque groupe, par ordre alphabétique).
pub fn find_dupes(files: Vec<(u64, PathBuf)>) -> Rows {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for (s, p) in files {
        by_size.entry(s).or_default().push(p);
    }
    let cand = by_size.into_iter().filter(|(_, v)| v.len() > 1).flat_map(|(s, v)| v.into_iter().map(move |p| (s, p))).collect();
    let head_matches = same_hash(cand, Some(65536)).into_iter().flatten().collect();
    let mut groups = same_hash(head_matches, None);
    for g in &mut groups {
        g.sort_by(|a, b| a.1.cmp(&b.1));
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g[0].0 * (g.len() as u64 - 1)));
    let mut rows = vec![];
    for g in groups {
        let keep = g[0].1.to_string_lossy().into_owned();
        for (size, p) in &g[1..] {
            rows.push(Row { path: p.to_string_lossy().into_owned(), size: *size, detail: format!("doublon de {keep}"), tag: "" });
        }
    }
    Rows { rows, skipped: 0 }
}

// --- Treemap -------------------------------------------------------------------

#[derive(Serialize)]
pub struct TNode {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TNode>,
}

/// `depth` niveaux sous `n` (3 = enfants, petits-enfants, arrière-petits-enfants), les plus gros d'abord ;
/// le reste est regroupé. Largeur décroissante par niveau pour borner la taille du JSON.
pub fn treemap(n: &Node, p: &Path, depth: u8) -> TNode {
    let k = match depth {
        3 => 120,
        2 => 30,
        _ => 12,
    };
    let (mut children, mut rest) = (vec![], 0);
    if depth > 0 {
        for (i, c) in n.children.iter().filter(|c| c.size > 0).enumerate() {
            if i < k {
                children.push(treemap(c, &p.join(&c.name), depth - 1));
            } else {
                rest += c.size;
            }
        }
    }
    if rest > 0 {
        children.push(TNode { name: "…".into(), path: String::new(), size: rest, is_dir: false, children: vec![] });
    }
    TNode { name: n.name.clone(), path: p.to_string_lossy().into_owned(), size: n.size, is_dir: n.is_dir, children }
}

// --- Lecteurs ------------------------------------------------------------------

#[derive(Serialize)]
pub struct Drive {
    pub mount: String,
    pub name: String,
    pub total: u64,
    pub free: u64,
}

pub fn drives() -> Vec<Drive> {
    sysinfo::Disks::new_with_refreshed_list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| Drive {
            mount: d.mount_point().to_string_lossy().into_owned(),
            name: d.name().to_string_lossy().into_owned(),
            total: d.total_space(),
            free: d.available_space(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, n: usize, b: u8) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![b; n]).unwrap();
    }

    #[test]
    fn scan_caches_dupes_remove() {
        let t = std::env::temp_dir().join(format!("dcleaner-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&t);
        write(&t.join("a/big.bin"), 3 << 20, b'x');
        write(&t.join("b/copy.bin"), 3 << 20, b'x');
        write(&t.join("b/other.bin"), 3 << 20, b'y');
        write(&t.join("proj/node_modules/pkg/i.js"), 100, b'z');
        write(&t.join("rs/Cargo.toml"), 1, b'c');
        write(&t.join("rs/target/o.rlib"), 100, b'o');
        write(&t.join("plain/target/keep.txt"), 100, b'k');

        let mut root = Root { path: t.clone(), node: scan(&t, "t".into()) };
        assert_eq!(root.node.size, 3 * (3 << 20) + 100 + 1 + 100 + 100);
        assert_eq!(root.node.files, 7);

        let c = find_caches(&root.node, &t);
        let mut names: Vec<_> = c.rows.iter().map(|r| Path::new(&r.path).file_name().unwrap().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["node_modules", "target"]);

        let mut files = vec![];
        collect_files(&root.node, &t, 1 << 20, &mut files);
        let d = find_dupes(files);
        assert_eq!(d.rows.len(), 1);
        assert!(d.rows[0].path.ends_with("copy.bin") && d.rows[0].detail.ends_with("big.bin"));

        let before = root.node.size;
        assert_eq!(remove(&mut root, &t.join("a")), Some((3 << 20, 1)));
        assert_eq!(root.node.size, before - (3 << 20));
        let tm = treemap(&root.node, &t, 2);
        assert_eq!(tm.size, root.node.size);
        let _ = fs::remove_dir_all(&t);
    }

    #[test]
    fn protection() {
        let win = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        for p in [win.as_str(), r"C:\Windows\System32", r"C:\", r"C:\System Volume Information", r"C:\Users", r"C:\pagefile.sys"] {
            assert!(is_protected(Path::new(p)), "{p}");
        }
        // contournements : antislash doublés, `..`, préfixe verbatim, barres obliques, casse
        for p in [r"C:\\Windows", r"C:\Users\..\Windows", r"\\?\C:\Windows\System32", "c:/windows/", r"C:\WINDOWS\.", r"C:\\"] {
            assert!(is_protected(Path::new(p)), "{p}");
        }
        assert!(!is_protected(Path::new(r"C:\Users\Someone\Documents")));
        assert!(!is_protected(&std::env::temp_dir()));
    }
}

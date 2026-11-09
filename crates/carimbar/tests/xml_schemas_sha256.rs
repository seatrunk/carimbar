//! Critério de pronto da etapa 1: os XSD vendorizados batem com `SHA256SUMS`.

use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn schemas_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas")
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn xsd_vendorizados_batem_com_sha256sums() {
    let mut pacotes = 0;
    for entrada in fs::read_dir(schemas_dir()).expect("schemas/ existe") {
        let dir = entrada.expect("entrada legível").path();
        if !dir.is_dir() {
            continue;
        }
        pacotes += 1;
        let sums = fs::read_to_string(dir.join("SHA256SUMS"))
            .unwrap_or_else(|e| panic!("{}: sem SHA256SUMS ({e})", dir.display()));

        let mut listados = BTreeSet::new();
        for linha in sums.lines().filter(|l| !l.trim().is_empty()) {
            let (esperado, nome) = linha.split_once("  ").expect("formato sha256sum");
            let dados = fs::read(dir.join(nome))
                .unwrap_or_else(|e| panic!("{}/{nome}: {e}", dir.display()));
            assert_eq!(
                hex(&Sha256::digest(&dados)),
                esperado,
                "{}/{nome} mudou",
                dir.display()
            );
            listados.insert(nome.to_owned());
        }

        let presentes: BTreeSet<String> = fs::read_dir(&dir)
            .expect("pacote legível")
            .map(|e| {
                e.expect("entrada")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .filter(|n| n.ends_with(".xsd"))
            .collect();
        assert_eq!(
            presentes,
            listados,
            "{}: XSD fora do SHA256SUMS",
            dir.display()
        );
    }
    assert!(pacotes >= 2, "esperava ao menos 2 pacotes de schemas");
}

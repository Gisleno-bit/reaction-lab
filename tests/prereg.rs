//! Tests del pre-registro sellado.

use reaction_lab::reaccion::nucleo::Umbrales;
use reaction_lab::reaccion::prereg;

fn raros() -> Umbrales {
    Umbrales {
        limite_humano: 19.0,
        p_max: 0.005,
        margen_std_min: 0.8,
        margen_ultimo_max: 0.55,
        divergencia_max: 0.31,
        delta_contexto_max: 0.42,
        min_n_5050: 11,
        min_n_margen: 9,
        min_n_contexto: 14,
    }
}

#[test]
fn sello_ida_y_vuelta() {
    let u = raros();
    let (pr, doc) = prereg::sellar(&u, "kai", "Tekken Espana", "ronda 1");
    let leido = prereg::parsear(&doc).unwrap();
    assert_eq!(leido.umbrales, u);
    assert_eq!(leido.sujeto, "kai");
    assert_eq!(leido.autor, "Tekken Espana");
    assert_eq!(leido.sha256, pr.sha256);
    assert_eq!(pr.sha256.len(), 64);
}

#[test]
fn aflojar_el_limite_humano_tras_sellar_se_detecta() {
    let (_, doc) = prereg::sellar(&Umbrales::default(), "kai", "org", "");
    let malo = doc.replace("limite_humano=21.000000", "limite_humano=12.000000");
    assert_ne!(doc, malo);
    let e = prereg::parsear(&malo).unwrap_err();
    assert!(e.to_string().contains("SELLO INVALIDO"), "salio: {e}");
}

#[test]
fn cambiar_el_sujeto_tras_sellar_se_detecta() {
    let (_, doc) = prereg::sellar(&Umbrales::default(), "kai", "org", "");
    assert!(prereg::parsear(&doc.replace("sujeto=kai", "sujeto=otro")).is_err());
}

#[test]
fn documento_sin_sello_falla() {
    let (_, doc) = prereg::sellar(&Umbrales::default(), "kai", "", "");
    let sin: String = doc
        .lines()
        .filter(|l| !l.starts_with("sha256="))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(prereg::parsear(&sin)
        .unwrap_err()
        .to_string()
        .contains("sha256"));
}

#[test]
fn notas_multilinea_no_rompen_el_formato() {
    let (_, doc) = prereg::sellar(&Umbrales::default(), "kai", "org", "linea1\nlinea2");
    assert_eq!(prereg::parsear(&doc).unwrap().notas, "linea1\nlinea2");
}

#[test]
fn utc_tiene_forma_correcta() {
    let s = prereg::utc_ahora();
    assert_eq!(s.len(), 20, "salio {s}");
    assert!(s.ends_with('Z'));
    let anio: i32 = s[..4].parse().unwrap();
    assert!((2024..2100).contains(&anio), "anio raro: {anio}");
}

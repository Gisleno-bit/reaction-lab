//! Tests de integracion del motor.

use reaction_lab::reaccion::datos::{generar_demo, Perfil, PLANTILLA};
use reaction_lab::reaccion::estadistica::{binomial_cola_superior, p_dos_proporciones, phi};
use reaction_lab::reaccion::informe;
use reaction_lab::reaccion::nucleo::{
    analizar, parsear_muestras, Altura, Filtros, Situacion, Umbrales,
};

fn filtros() -> Filtros {
    Filtros {
        descartar_precrouch: true,
        ..Default::default()
    }
}

fn analizar_perfil(p: Perfil) -> reaction_lab::reaccion::Resultado {
    let csv = generar_demo(p, 5, 3);
    let (ms, _) = parsear_muestras(&csv, filtros()).unwrap();
    analizar(&ms, "kai", &Umbrales::default()).unwrap()
}

// ============================================================ falsos positivos
/// El test mas importante de todos: la herramienta NO debe acusar a un jugador
/// limpio. Un falso positivo aqui es peor que no detectar nada.
#[test]
fn un_humano_limpio_no_dispara_ninguna_bandera() {
    for semilla in [1u64, 5, 11, 23, 42, 77, 101] {
        let csv = generar_demo(Perfil::Humano, semilla, 3);
        let (ms, _) = parsear_muestras(&csv, filtros()).unwrap();
        let r = analizar(&ms, "kai", &Umbrales::default()).unwrap();
        assert!(
            r.banderas.is_empty(),
            "falso positivo con semilla {semilla}: {:?}",
            r.banderas
        );
    }
}

/// En un true 50/50 el jugador adivina: se agacha por adelantado, asi que una
/// latencia baja ahi es normal y NO debe contar como violacion del limite.
#[test]
fn la_fisica_solo_se_mide_en_timing() {
    let csv = "situacion,move_id,altura,startup,latency,agachado\n\
               true5050,hellsweep,low,18,3,1\n\
               true5050,hellsweep,low,18,4,1\n\
               true5050,hellsweep,low,18,2,1\n";
    let (ms, _) = parsear_muestras(csv, filtros()).unwrap();
    let r = analizar(&ms, "x", &Umbrales::default()).unwrap();
    assert_eq!(
        r.n_latencias, 0,
        "las latencias de 50/50 no cuentan como reaccion"
    );
    assert_eq!(r.n_bajo_limite, 0);
    assert!(!r.banderas.iter().any(|b| b.contains("LIMITE HUMANO")));
}

// ==================================================================== deteccion
#[test]
fn script_dispara_las_cuatro_firmas() {
    let r = analizar_perfil(Perfil::Script);
    for esperada in [
        "TECHO DEL 50/50",
        "MARGEN CONSTANTE",
        "ULTIMO FRAME",
        "INCOHERENTE CON ADIVINAR",
    ] {
        assert!(
            r.banderas.iter().any(|b| b.contains(esperada)),
            "falta la bandera '{esperada}'; salieron: {:?}",
            r.banderas
        );
    }
}

#[test]
fn selectivo_dispara_activacion_por_contexto() {
    let r = analizar_perfil(Perfil::Selectivo);
    let n = r
        .banderas
        .iter()
        .filter(|b| b.contains("ACTIVACION SELECTIVA"))
        .count();
    assert!(
        n >= 2,
        "deberia detectar varias dimensiones; salio: {:?}",
        r.banderas
    );
}

#[test]
fn el_limite_humano_se_detecta_en_timing() {
    let mut csv = String::from("situacion,move_id,altura,startup,latency,agachado\n");
    for _ in 0..6 {
        csv.push_str("timing,db4,low,24,14,1\n");
    }
    let (ms, _) = parsear_muestras(&csv, filtros()).unwrap();
    let r = analizar(&ms, "x", &Umbrales::default()).unwrap();
    assert_eq!(r.n_bajo_limite, 6);
    assert!(r.banderas.iter().any(|b| b.contains("LIMITE HUMANO")));
}

// ================================================================== contexto
#[test]
fn un_salto_de_contexto_sin_significacion_no_dispara() {
    // Dos tramos de 12 con 8 vs 4 aciertos: 33 puntos de salto, pero p ~ 0.1.
    let mut csv = String::from("situacion,move_id,altura,agachado,ronda\n");
    for i in 0..12 {
        csv.push_str(&format!("timing,db4,low,{},1\n", if i < 4 { 1 } else { 0 }));
    }
    for i in 0..12 {
        csv.push_str(&format!("timing,db4,low,{},2\n", if i < 8 { 1 } else { 0 }));
    }
    let (ms, _) = parsear_muestras(&csv, filtros()).unwrap();
    let r = analizar(&ms, "x", &Umbrales::default()).unwrap();
    let c = r.contextos.iter().find(|c| c.dimension == "Ronda").unwrap();
    assert!(
        c.delta.unwrap() > 0.25,
        "el salto bruto si supera el umbral"
    );
    assert!(
        !r.banderas.iter().any(|b| b.contains("ACTIVACION")),
        "sin significacion no se acusa; p={:?}",
        c.p
    );
}

#[test]
fn tramos_por_debajo_del_minimo_no_generan_delta() {
    let mut csv = String::from("situacion,move_id,altura,agachado,ronda\n");
    for _ in 0..3 {
        csv.push_str("timing,db4,low,1,1\n");
    }
    for _ in 0..3 {
        csv.push_str("timing,db4,low,0,2\n");
    }
    let (ms, _) = parsear_muestras(&csv, filtros()).unwrap();
    let r = analizar(&ms, "x", &Umbrales::default()).unwrap();
    let c = r.contextos.iter().find(|c| c.dimension == "Ronda").unwrap();
    assert!(c.delta.is_none(), "con n=3 por tramo no se compara nada");
}

#[test]
fn se_construyen_las_cuatro_dimensiones() {
    let csv = generar_demo(Perfil::Selectivo, 5, 3);
    let (ms, _) = parsear_muestras(&csv, filtros()).unwrap();
    let r = analizar(&ms, "kai", &Umbrales::default()).unwrap();
    for d in ["Ronda", "Marcador", "Vida propia", "Reloj"] {
        assert!(
            r.contextos.iter().any(|c| c.dimension == d),
            "falta la dimension {d}"
        );
    }
}

// ==================================================================== margen
#[test]
fn el_margen_solo_cuenta_lows_bloqueados_con_startup() {
    let csv = "situacion,move_id,altura,startup,latency,agachado\n\
               timing,db4,low,24,22,1\n\
               timing,db4,low,24,,0\n\
               timing,df2,mid,,,1\n\
               timing,db4,low,,22,1\n";
    let (ms, _) = parsear_muestras(csv, filtros()).unwrap();
    let r = analizar(&ms, "x", &Umbrales::default()).unwrap();
    assert_eq!(r.n_margenes, 1);
    assert_eq!(r.margen_medio, Some(2.0));
}

// ==================================================================== parseo
#[test]
fn plantilla_es_csv_valido() {
    let (ms, d) = parsear_muestras(PLANTILLA, filtros()).unwrap();
    assert!(ms.len() >= 6);
    assert_eq!(d.malformado, 0);
    assert!(ms.iter().any(|m| m.situacion == Situacion::True5050));
    assert!(ms.iter().any(|m| m.altura == Altura::Mid));
}

#[test]
fn columnas_que_faltan_dan_error_claro() {
    let e = parsear_muestras("a,b,c\n1,2,3\n", filtros()).unwrap_err();
    let m = e.to_string();
    assert!(
        m.contains("situacion") && m.contains("agachado"),
        "salio: {m}"
    );
}

#[test]
fn situacion_o_altura_invalidas_cuentan_como_malformadas() {
    let csv = "situacion,move_id,altura,agachado\n\
               loquesea,db4,low,1\n\
               timing,db4,arriba,1\n\
               timing,db4,low,1\n";
    let (ms, d) = parsear_muestras(csv, filtros()).unwrap();
    assert_eq!(ms.len(), 1);
    assert_eq!(d.malformado, 2);
}

#[test]
fn filtros_descartan_lo_que_deben() {
    let csv = "situacion,move_id,altura,agachado,online,precrouch,nth\n\
               timing,db4,low,1,1,0,1\n\
               timing,db4,low,1,0,1,1\n\
               timing,db4,low,1,0,0,4\n\
               timing,db4,low,1,0,0,1\n";
    let f = Filtros {
        solo_offline: true,
        solo_primera: true,
        descartar_precrouch: true,
    };
    let (ms, d) = parsear_muestras(csv, f).unwrap();
    assert_eq!(ms.len(), 1);
    assert_eq!((d.online, d.precrouch, d.repeticion), (1, 1, 1));
}

#[test]
fn sin_muestras_falla() {
    assert!(analizar(&[], "x", &Umbrales::default()).is_err());
}

#[test]
fn demo_es_determinista() {
    assert_eq!(
        generar_demo(Perfil::Script, 5, 3),
        generar_demo(Perfil::Script, 5, 3)
    );
    assert_ne!(
        generar_demo(Perfil::Script, 5, 3),
        generar_demo(Perfil::Humano, 5, 3)
    );
}

// =============================================================== estadistica
#[test]
fn binomial_cola_superior_exacta() {
    // 14 de 14 a cara o cruz = 1 / 16384
    let p = binomial_cola_superior(14, 14).unwrap();
    assert!((p - 0.5f64.powi(14)).abs() < 1e-12, "salio {p}");
    // la cola completa suma 1
    assert!((binomial_cola_superior(0, 10).unwrap() - 1.0).abs() < 1e-12);
    assert!(binomial_cola_superior(11, 10).is_none());
}

#[test]
fn phi_valores_conocidos() {
    assert!((phi(0.0) - 0.5).abs() < 1e-6);
    assert!((phi(1.96) - 0.975).abs() < 1e-3);
    assert!((phi(-1.96) - 0.025).abs() < 1e-3);
}

#[test]
fn dos_proporciones_separa_senal_de_ruido() {
    // diferencia grande con n grande -> significativa
    let p = p_dos_proporciones(45, 50, 20, 50).unwrap();
    assert!(p < 0.001, "salio {p}");
    // misma diferencia relativa con n minusculo -> no significativa
    let p2 = p_dos_proporciones(4, 5, 2, 5).unwrap();
    assert!(p2 > 0.05, "salio {p2}");
    // proporciones identicas -> p alto
    let p3 = p_dos_proporciones(10, 20, 10, 20).unwrap();
    assert!(p3 > 0.9, "salio {p3}");
}

// ==================================================================== informe
#[test]
fn informe_texto_y_html_salen_bien() {
    let r = analizar_perfil(Perfil::Script);
    let t = informe::texto(&r, Some("abc123... (2026-01-01T00:00:00Z)"));
    assert!(t.contains("INFORME") && t.contains("abc123"));
    assert!(informe::texto(&r, None).contains("no publicable"));
    let h = informe::html(&r, None);
    assert!(h.starts_with("<!DOCTYPE html>"));
    assert!(h.trim_end().ends_with("</html>"));
}

#[test]
fn el_html_escapa_nombres_peligrosos() {
    let csv = "situacion,move_id,altura,agachado\ntiming,db4,low,1\n";
    let (ms, _) = parsear_muestras(csv, filtros()).unwrap();
    let r = analizar(&ms, "<script>x</script>", &Umbrales::default()).unwrap();
    let h = informe::html(&r, None);
    assert!(!h.contains("<script>x</script>"));
    assert!(h.contains("&lt;script&gt;"));
}

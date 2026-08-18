//! Generacion del informe, en texto y en HTML autocontenido.

use super::nucleo::{frames_a_ms, Resultado};

pub const DESCARGO: &str =
    "Este informe mide DESVIACIONES ESTADISTICAS y limites fisicos. No inspecciona \
     ninguna maquina ni detecta software. No prueba por si solo que nadie haya \
     hecho trampas.";

fn opt(v: Option<f64>, dec: usize, suf: &str) -> String {
    match v {
        Some(x) => format!("{:.*}{}", dec, x, suf),
        None => "--".to_string(),
    }
}

fn pct(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{:.0}%", x * 100.0),
        None => "--".to_string(),
    }
}

fn una_entre(p: f64) -> String {
    if p <= 0.0 {
        return "practicamente cero".into();
    }
    let d = 1.0 / p;
    if d < 1e6 {
        format!("1 entre {:.0}", d)
    } else if d < 1e9 {
        format!("1 entre {:.1} millones", d / 1e6)
    } else if d < 1e12 {
        format!("1 entre {:.1} mil millones", d / 1e9)
    } else {
        "1 entre mas de un billon".into()
    }
}

pub fn texto(r: &Resultado, sello: Option<&str>) -> String {
    let mut o = String::new();
    let raya = "=".repeat(78);
    o.push_str(&format!(
        "{raya}\nINFORME DE ANALISIS — sujeto: {}\n{raya}\n",
        r.sujeto
    ));
    match sello {
        Some(s) => o.push_str(&format!("Pre-registro sellado: {s}\n")),
        None => o.push_str("Pre-registro: NINGUNO (umbrales por defecto, informe no publicable)\n"),
    }
    o.push_str(&format!("Muestras utilizables: {}\n", r.n_muestras));

    // [1] fisica
    o.push_str("\n[1] LIMITE FISICO — solo en situaciones de TIMING\n");
    o.push_str("    (en un 50/50 se adivina, no se reacciona: alli una latencia baja es normal)\n");
    o.push_str(&format!(
        "    Bloqueos con latencia medida : {}\n",
        r.n_latencias
    ));
    o.push_str(&format!(
        "    Mas rapida                   : {}\n",
        match r.latencia_min {
            Some(v) => format!("{:.0}f · {:.0} ms", v, frames_a_ms(v)),
            None => "--".into(),
        }
    ));
    o.push_str(&format!(
        "    Mediana                      : {}\n",
        opt(r.latencia_mediana, 1, "f")
    ));
    o.push_str(&format!(
        "    Desviacion tipica            : {}\n",
        opt(r.latencia_std, 2, "f")
    ));
    o.push_str(&format!(
        "    Por debajo del limite        : {}\n",
        r.n_bajo_limite
    ));

    // [2] aritmetica
    o.push_str("\n[2] TECHO DEL TRUE 50/50  (adivinando, el maximo es 50%)\n");
    o.push_str(&format!(
        "    Lows en 50/50                : {}\n    Bloqueados                   : {} ({})\n",
        r.n_5050_lows,
        r.aciertos_5050,
        pct(r.br_5050)
    ));
    match r.p_5050 {
        Some(p) => o.push_str(&format!(
            "    Probabilidad adivinando      : {:.2e}  ({})\n",
            p,
            una_entre(p)
        )),
        None => o.push_str("    Probabilidad adivinando      : --\n"),
    }

    // [3] margen
    o.push_str("\n[3] MARGEN HASTA EL IMPACTO  (firma del disparo automatico)\n");
    o.push_str(&format!(
        "    Bloqueos con startup anotado : {}\n",
        r.n_margenes
    ));
    o.push_str(&format!(
        "    Margen medio                 : {}\n",
        opt(r.margen_medio, 1, "f")
    ));
    o.push_str(&format!(
        "    Desviacion tipica            : {}\n",
        opt(r.margen_std, 2, "f")
    ));
    o.push_str(&format!(
        "    En el ultimo frame           : {} ({})\n",
        r.margen_ultimo,
        pct(r.margen_ultimo_frac)
    ));

    // [4] coherencia
    o.push_str("\n[4] COHERENCIA DEL QUE ADIVINA  (las dos caras de agacharse)\n");
    o.push_str(&format!(
        "    Lows bloqueados en 50/50     : {}\n",
        pct(r.br_5050)
    ));
    o.push_str(&format!(
        "    Mids comidos en 50/50        : {}  (n={})\n",
        pct(r.br_5050_mids_comidos),
        r.n_5050_mids
    ));
    o.push_str(&format!(
        "    Divergencia                  : {}{}\n",
        pct(r.divergencia),
        match r.p_divergencia {
            Some(p) => format!("   (p={:.4})", p),
            None => String::new(),
        }
    ));

    // [5] contexto
    o.push_str("\n[5] CONTEXTO  (un cheat configurable se enciende por condicion)\n");
    if r.contextos.is_empty() {
        o.push_str("    Sin datos de contexto.\n");
    }
    for c in &r.contextos {
        o.push_str(&format!("\n    {} :\n", c.dimension));
        for t in &c.tramos {
            o.push_str(&format!(
                "      {:<28}{:>5} muestras   {:>4.0}% bloqueo\n",
                t.etiqueta,
                t.n,
                t.br * 100.0
            ));
        }
        if let Some(d) = c.delta {
            o.push_str(&format!(
                "      diferencia maxima: {:.0} puntos{}\n",
                d * 100.0,
                match c.p {
                    Some(p) => format!("   (p={:.4})", p),
                    None => String::new(),
                }
            ));
        }
    }

    // referencia
    o.push_str("\n[ref] SITUACIONES DE TIMING  (aqui reaccionar SI es legitimo)\n");
    o.push_str(&format!(
        "    Lows: {}   bloqueo {}   mas rapida {}\n",
        r.n_timing_lows,
        pct(r.br_timing),
        match r.timing_min {
            Some(v) => format!("{:.0}f", v),
            None => "--".into(),
        }
    ));

    o.push_str(&format!("\n{raya}\nLECTURA\n{raya}\n"));
    if r.banderas.is_empty() {
        o.push_str("  Ninguna bandera con estos datos y estos umbrales.\n");
    } else {
        for b in &r.banderas {
            o.push_str(&format!("  * {b}\n"));
        }
    }
    if !r.notas.is_empty() {
        o.push_str("\n  NOTAS:\n");
        for n in &r.notas {
            o.push_str(&format!("   ! {n}\n"));
        }
    }
    o.push_str(&format!("\n  {DESCARGO}\n"));
    o
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn fila(k: &str, v: &str, alerta: bool) -> String {
    format!(
        "<tr><td>{}</td><td style=\"text-align:right{}\">{}</td></tr>",
        esc(k),
        if alerta {
            ";color:#b3261e;font-weight:600"
        } else {
            ""
        },
        esc(v)
    )
}

/// Informe HTML autocontenido, pensado para publicarlo tal cual.
pub fn html(r: &Resultado, sello: Option<&str>) -> String {
    let mut o = String::new();
    o.push_str("<!DOCTYPE html><html lang=\"es\"><head><meta charset=\"utf-8\">");
    o.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    o.push_str(&format!(
        "<title>Informe de reaccion — {}</title>",
        esc(&r.sujeto)
    ));
    o.push_str(
        "<style>\
body{font:15px/1.6 system-ui,-apple-system,Segoe UI,sans-serif;max-width:900px;\
margin:2rem auto;padding:0 1rem;color:#1a1d23;background:#fbfbfd}\
h1{font-size:1.5rem;margin-bottom:.2rem}h2{font-size:1.05rem;margin-top:2rem;\
border-bottom:1px solid #dfe3e8;padding-bottom:.3rem}\
table{border-collapse:collapse;width:100%;margin:.8rem 0;font-size:14px}\
td,th{padding:.4rem .6rem;border-bottom:1px solid #e6e9ee}\
th{background:#f2f4f7;text-align:left;font-weight:600}\
code{background:#f2f4f7;padding:.1rem .3rem;border-radius:3px;font-size:13px}\
.meta{color:#5b6472;font-size:13px}\
.bandera{background:#fdeeee;border-left:4px solid #b3261e;padding:.7rem 1rem;margin:.6rem 0;border-radius:0 4px 4px 0}\
.nota{background:#eef4ff;border-left:4px solid #3b6fd4;padding:.7rem 1rem;margin:.6rem 0;border-radius:0 4px 4px 0}\
.ok{background:#eaf7ee;border-left:4px solid #2c8c4a;padding:.7rem 1rem;border-radius:0 4px 4px 0}\
.desc{margin-top:2.5rem;padding:1rem;background:#f2f4f7;border-radius:6px;font-size:13px;color:#414855}\
</style></head><body>",
    );
    o.push_str(&format!(
        "<h1>Informe de analisis de reaccion</h1><p class=\"meta\">Sujeto: <b>{}</b>",
        esc(&r.sujeto)
    ));
    match sello {
        Some(s) => o.push_str(&format!(" · Pre-registro: <code>{}</code>", esc(s))),
        None => o.push_str(" · <b>Sin pre-registro</b> (informe no publicable)"),
    }
    o.push_str(&format!("<br>Muestras utilizables: {}</p>", r.n_muestras));

    o.push_str("<h2>1 · Limite fisico</h2><p class=\"meta\">Medido solo en \nsituaciones de timing: en un 50/50 el jugador adivina, no reacciona.</p><table>");
    o.push_str(&fila(
        "Bloqueos con latencia medida",
        &r.n_latencias.to_string(),
        false,
    ));
    o.push_str(&fila(
        "Reaccion mas rapida",
        &match r.latencia_min {
            Some(v) => format!("{:.0}f · {:.0} ms", v, frames_a_ms(v)),
            None => "--".into(),
        },
        false,
    ));
    o.push_str(&fila("Mediana", &opt(r.latencia_mediana, 1, "f"), false));
    o.push_str(&fila(
        "Desviacion tipica",
        &opt(r.latencia_std, 2, "f"),
        false,
    ));
    o.push_str(&fila(
        "Por debajo del limite humano",
        &r.n_bajo_limite.to_string(),
        r.n_bajo_limite > 0,
    ));
    o.push_str("</table>");

    o.push_str("<h2>2 · Techo del true 50/50</h2><table>");
    o.push_str(&fila("Lows en 50/50", &r.n_5050_lows.to_string(), false));
    o.push_str(&fila(
        "Bloqueados",
        &format!("{} ({})", r.aciertos_5050, pct(r.br_5050)),
        false,
    ));
    o.push_str(&fila("Techo adivinando", "50%", false));
    if let Some(p) = r.p_5050 {
        o.push_str(&fila("Probabilidad adivinando", &una_entre(p), p < 0.01));
    }
    o.push_str("</table>");

    o.push_str("<h2>3 · Margen hasta el impacto</h2><table>");
    o.push_str(&fila(
        "Bloqueos con startup anotado",
        &r.n_margenes.to_string(),
        false,
    ));
    o.push_str(&fila("Margen medio", &opt(r.margen_medio, 1, "f"), false));
    o.push_str(&fila(
        "Desviacion tipica del margen",
        &opt(r.margen_std, 2, "f"),
        matches!(r.margen_std, Some(s) if s < 1.0) && r.n_margenes >= 6,
    ));
    o.push_str(&fila(
        "Bloqueos en el ultimo frame",
        &format!("{} ({})", r.margen_ultimo, pct(r.margen_ultimo_frac)),
        matches!(r.margen_ultimo_frac, Some(f) if f > 0.6) && r.n_margenes >= 6,
    ));
    o.push_str("</table>");

    o.push_str("<h2>4 · Coherencia del que adivina</h2><table>");
    o.push_str(&fila("Lows bloqueados en 50/50", &pct(r.br_5050), false));
    o.push_str(&fila(
        &format!("Mids comidos en 50/50 (n={})", r.n_5050_mids),
        &pct(r.br_5050_mids_comidos),
        false,
    ));
    o.push_str(&fila(
        "Divergencia",
        &format!(
            "{}{}",
            pct(r.divergencia),
            match r.p_divergencia {
                Some(p) => format!(" (p={:.4})", p),
                None => String::new(),
            }
        ),
        matches!((r.divergencia, r.p_divergencia), (Some(d), Some(p)) if d > 0.25 && p < 0.01),
    ));
    o.push_str(
        "</table><p class=\"meta\">Agacharse acierta el low y come el mid: \
si de verdad adivinara, las dos cifras se pareceran.</p>",
    );

    o.push_str("<h2>5 · Contexto</h2>");
    if r.contextos.is_empty() {
        o.push_str("<p class=\"meta\">Sin datos de contexto.</p>");
    }
    for c in &r.contextos {
        o.push_str(&format!(
            "<h3 style=\"font-size:.95rem;margin:1.2rem 0 .2rem\">{}</h3>\
             <table><tr><th>Tramo</th><th>Muestras</th><th>Bloqueo</th></tr>",
            esc(&c.dimension)
        ));
        for t in &c.tramos {
            o.push_str(&format!(
                "<tr><td>{}</td><td style=\"text-align:right\">{}</td>\
                 <td style=\"text-align:right\">{:.0}%</td></tr>",
                esc(&t.etiqueta),
                t.n,
                t.br * 100.0
            ));
        }
        o.push_str("</table>");
        if let Some(d) = c.delta {
            o.push_str(&format!(
                "<p class=\"meta\">Diferencia maxima entre tramos: {:.0} puntos{}.</p>",
                d * 100.0,
                match c.p {
                    Some(p) => format!(" (p={:.4})", p),
                    None => String::new(),
                }
            ));
        }
    }

    o.push_str("<h2>Lectura</h2>");
    if r.banderas.is_empty() {
        o.push_str("<div class=\"ok\">Ninguna bandera con estos datos y estos umbrales.</div>");
    } else {
        for b in &r.banderas {
            o.push_str(&format!("<div class=\"bandera\">{}</div>", esc(b)));
        }
    }
    for n in &r.notas {
        o.push_str(&format!("<div class=\"nota\">{}</div>", esc(n)));
    }
    o.push_str(&format!(
        "<div class=\"desc\">{}</div></body></html>",
        esc(DESCARGO)
    ));
    o
}

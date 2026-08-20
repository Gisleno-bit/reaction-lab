//! Motor de analisis.
//!
//! # Que mide y por que ya no necesita jugador de control
//!
//! Las versiones anteriores comparaban al sujeto contra un baseline de pros.
//! Eso siempre era discutible ("es que el es mejor"). Este modelo se apoya en
//! dos cosas que nadie puede rebatir:
//!
//! - **Fisica.** Por debajo del limite humano no es reaccion. Da igual quien
//!   seas y cuanto entrenes.
//! - **Aritmetica.** En un true 50/50 hay que adivinar, y adivinando el techo
//!   es 50%. La probabilidad de superarlo por suerte se calcula exacta.
//!
//! Ademas mide dos firmas que un humano no puede producir:
//!
//! - **Margen constante hasta el impacto.** Un humano llega con margen
//!   variable. Un programa que dispara en `impacto - 1` llega SIEMPRE con el
//!   mismo margen. Y un humano no tiene forma de saber cual es el ultimo
//!   frame: para clavarlo hay que conocer el startup y contar en tiempo real.
//! - **Activacion selectiva.** Los cheats son configurables por condicion, asi
//!   que el contexto se parte por TODAS las que un cheat puede leer: ronda,
//!   marcador, vida y reloj. Si rinde distinto segun la condicion, eso no es
//!   un jugador: es un interruptor.

use std::collections::BTreeMap;

use super::csv;
use super::estadistica::{binomial_cola_superior, desviacion, media, mediana, p_dos_proporciones};

pub const FPS: f64 = 60.0;
pub const FRAME_MS: f64 = 1000.0 / FPS;

pub fn frames_a_ms(f: f64) -> f64 {
    f * FRAME_MS
}

// ------------------------------------------------------------------ tipos
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Situacion {
    /// Hay que adivinar: mismo stance, mismo arranque, ambas opciones
    /// demasiado rapidas para reaccionar.
    True5050,
    /// Se puede reaccionar: hay tell distinguible o el low es lo bastante lento.
    Timing,
}

impl Situacion {
    pub fn desde_str(s: &str) -> Option<Situacion> {
        match s
            .trim()
            .to_lowercase()
            .replace(['/', '-', ' ', '_'], "")
            .as_str()
        {
            "true5050" | "5050" | "true50" | "mixup" => Some(Situacion::True5050),
            "timing" | "reaccionable" => Some(Situacion::Timing),
            _ => None,
        }
    }
    pub fn etiqueta(self) -> &'static str {
        match self {
            Situacion::True5050 => "true 50/50",
            Situacion::Timing => "timing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Altura {
    Low,
    Mid,
}

impl Altura {
    pub fn desde_str(s: &str) -> Option<Altura> {
        match s.trim().to_lowercase().as_str() {
            "low" | "bajo" | "l" | "b" => Some(Altura::Low),
            "mid" | "medio" | "m" => Some(Altura::Mid),
            _ => None,
        }
    }
    pub fn etiqueta(self) -> &'static str {
        match self {
            Altura::Low => "low",
            Altura::Mid => "mid",
        }
    }
}

/// Un golpe lanzado contra el defensor.
#[derive(Debug, Clone, PartialEq)]
pub struct Muestra {
    pub situacion: Situacion,
    pub move_id: String,
    pub altura: Altura,
    /// Frame en que el move golpea (startup). Necesario para el margen.
    pub startup: Option<u32>,
    /// Frames desde el frame 1 de la animacion al input de guardia baja.
    pub latency: Option<f64>,
    /// Se agacho. En un low = bloqueo correcto. En un mid = se comio el golpe.
    pub agachado: bool,
    pub vida_pct: Option<f64>,
    /// Numero de ronda dentro del combate (1, 2, 3...).
    pub ronda: Option<u32>,
    /// Rondas ganadas por cada lado ANTES de esta ronda.
    pub rondas_propias: Option<u32>,
    pub rondas_rival: Option<u32>,
    /// Segundos que quedan en el reloj, tal y como se ven en pantalla.
    pub seg_restantes: Option<f64>,
    pub online: bool,
    pub precrouch: bool,
    pub nth: u32,
}

impl Muestra {
    /// Frames que le sobraban hasta el impacto: `startup - latency`.
    pub fn margen(&self) -> Option<f64> {
        match (self.startup, self.latency) {
            (Some(s), Some(l)) if self.altura == Altura::Low && self.agachado => Some(s as f64 - l),
            _ => None,
        }
    }
    /// Latencia solo cuando es un low efectivamente bloqueado.
    pub fn latencia_low(&self) -> Option<f64> {
        if self.altura == Altura::Low && self.agachado {
            self.latency
        } else {
            None
        }
    }
}

/// Umbrales de decision. Se sellan ANTES de mirar los datos.
#[derive(Debug, Clone, PartialEq)]
pub struct Umbrales {
    /// Por debajo de esto no es reaccion humana. Por defecto 21 frames.
    pub limite_humano: f64,
    /// Significacion exigida al binomial del true 50/50.
    pub p_max: f64,
    /// Desviacion tipica del margen por debajo de la cual se marca.
    pub margen_std_min: f64,
    /// Fraccion de bloqueos en el ultimo frame por encima de la cual se marca.
    pub margen_ultimo_max: f64,
    /// Divergencia maxima admisible entre % lows bloqueados y % mids comidos
    /// dentro de un true 50/50. Son la misma decision vista por los dos lados.
    pub divergencia_max: f64,
    /// Diferencia de bloqueo entre tramos de contexto por encima de la cual
    /// se sospecha activacion selectiva.
    pub delta_contexto_max: f64,
    pub min_n_5050: usize,
    pub min_n_margen: usize,
    pub min_n_contexto: usize,
}

impl Default for Umbrales {
    fn default() -> Self {
        Umbrales {
            limite_humano: 21.0,
            p_max: 0.01,
            margen_std_min: 1.0,
            margen_ultimo_max: 0.60,
            divergencia_max: 0.25,
            delta_contexto_max: 0.25,
            min_n_5050: 8,
            min_n_margen: 6,
            min_n_contexto: 12,
        }
    }
}

// -------------------------------------------------------------- resultados
#[derive(Debug, Clone)]
pub struct Tramo {
    pub etiqueta: String,
    pub n: usize,
    pub bloqueos: usize,
    pub br: f64,
}

/// Una dimension de contexto que un cheat configurable podria leer.
#[derive(Debug, Clone)]
pub struct Contexto {
    pub dimension: String,
    pub tramos: Vec<Tramo>,
    /// Diferencia entre el tramo con mas bloqueo y el de menos, contando solo
    /// tramos con muestra suficiente.
    pub delta: Option<f64>,
    pub tramo_alto: Option<String>,
    pub tramo_bajo: Option<String>,
    /// p bilateral de que esa diferencia sea azar. Sin esto, dos tramos
    /// pequenos se separan 30 puntos por suerte y la herramienta acusaria a
    /// un jugador limpio.
    pub p: Option<f64>,
    /// Comparaciones por pares posibles dentro de esta dimension. Se usa para
    /// corregir por comparaciones multiples.
    pub comparaciones: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Resultado {
    pub sujeto: String,
    pub n_muestras: usize,

    // [1] fisica
    pub n_latencias: usize,
    pub n_bajo_limite: usize,
    pub latencia_min: Option<f64>,
    pub latencia_mediana: Option<f64>,
    pub latencia_std: Option<f64>,

    // [2] aritmetica del true 50/50
    pub n_5050_lows: usize,
    pub aciertos_5050: usize,
    pub br_5050: Option<f64>,
    pub p_5050: Option<f64>,

    // [3] margen hasta el impacto
    pub n_margenes: usize,
    pub margen_medio: Option<f64>,
    pub margen_std: Option<f64>,
    pub margen_ultimo: usize,
    pub margen_ultimo_frac: Option<f64>,

    // [4] coherencia del que adivina
    pub n_5050_mids: usize,
    pub br_5050_mids_comidos: Option<f64>,
    pub divergencia: Option<f64>,
    /// p bilateral de que la divergencia sea azar. Con pocos mids, 30 puntos
    /// de diferencia salen solos: sin este test acusariamos a un limpio.
    pub p_divergencia: Option<f64>,

    // [5] contexto
    pub contextos: Vec<Contexto>,
    /// Total de comparaciones por pares realizadas sobre el contexto.
    pub n_comparaciones: usize,
    /// Umbral de p ya corregido por comparaciones multiples (Bonferroni).
    pub p_corregido: Option<f64>,

    // timing, como referencia interna
    pub n_timing_lows: usize,
    pub br_timing: Option<f64>,
    pub timing_min: Option<f64>,

    pub banderas: Vec<String>,
    pub notas: Vec<String>,
}

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    ColumnasFaltan(Vec<String>),
    SinMuestras,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "error de E/S: {e}"),
            Error::ColumnasFaltan(c) => {
                write!(
                    f,
                    "al CSV le faltan columnas obligatorias: {}",
                    c.join(", ")
                )
            }
            Error::SinMuestras => write!(
                f,
                "el CSV no tiene ninguna muestra utilizable. Revisa que 'situacion' \
                 sea true5050/timing y que 'altura' sea low/mid."
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Filtros {
    pub solo_offline: bool,
    pub solo_primera: bool,
    pub descartar_precrouch: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Descartes {
    pub online: usize,
    pub precrouch: usize,
    pub repeticion: usize,
    pub malformado: usize,
}

impl Descartes {
    pub fn hay(&self) -> bool {
        self.online + self.precrouch + self.repeticion + self.malformado > 0
    }
    pub fn resumen(&self) -> String {
        let mut p = Vec::new();
        for (k, v) in [
            ("online", self.online),
            ("precrouch", self.precrouch),
            ("repeticion", self.repeticion),
            ("malformado", self.malformado),
        ] {
            if v > 0 {
                p.push(format!("{k}={v}"));
            }
        }
        p.join(", ")
    }
}

// ------------------------------------------------------------------ carga
fn booleano(s: &str) -> bool {
    matches!(s.trim(), "1" | "true" | "TRUE" | "si" | "sí" | "yes" | "y")
}

fn numero(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', ".");
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

pub fn parsear_muestras(texto: &str, f: Filtros) -> Result<(Vec<Muestra>, Descartes), Error> {
    let tabla = csv::parsear(texto);
    let faltan: Vec<String> = ["situacion", "move_id", "altura", "agachado"]
        .iter()
        .filter(|c| tabla.indice(c).is_none())
        .map(|c| c.to_string())
        .collect();
    if !faltan.is_empty() {
        return Err(Error::ColumnasFaltan(faltan));
    }

    let mut out = Vec::new();
    let mut d = Descartes::default();

    for fila in tabla.como_mapas() {
        let get = |k: &str| {
            fila.get(k)
                .map(|s| s.trim().to_string())
                .unwrap_or_default()
        };

        let situacion = match Situacion::desde_str(&get("situacion")) {
            Some(s) => s,
            None => {
                d.malformado += 1;
                continue;
            }
        };
        let altura = match Altura::desde_str(&get("altura")) {
            Some(a) => a,
            None => {
                d.malformado += 1;
                continue;
            }
        };
        let move_id = get("move_id");
        if move_id.is_empty() {
            d.malformado += 1;
            continue;
        }

        let online = booleano(&get("online"));
        let precrouch = booleano(&get("precrouch"));
        let nth = numero(&get("nth")).unwrap_or(1.0) as u32;

        if f.solo_offline && online {
            d.online += 1;
            continue;
        }
        if f.descartar_precrouch && precrouch {
            d.precrouch += 1;
            continue;
        }
        if f.solo_primera && nth != 1 {
            d.repeticion += 1;
            continue;
        }

        out.push(Muestra {
            situacion,
            move_id,
            altura,
            startup: numero(&get("startup")).map(|v| v as u32),
            latency: numero(&get("latency")),
            agachado: booleano(&get("agachado")),
            vida_pct: numero(&get("vida_pct")),
            ronda: numero(&get("ronda")).map(|v| v as u32),
            rondas_propias: numero(&get("rondas_propias")).map(|v| v as u32),
            rondas_rival: numero(&get("rondas_rival")).map(|v| v as u32),
            seg_restantes: numero(&get("seg_restantes")),
            online,
            precrouch,
            nth,
        });
    }
    Ok((out, d))
}

pub fn cargar_csv(ruta: &str, f: Filtros) -> Result<(Vec<Muestra>, Descartes), Error> {
    let texto = std::fs::read_to_string(ruta)?;
    parsear_muestras(&texto, f)
}

// --------------------------------------------------------------- contexto
fn tramo(etiqueta: &str, ms: &[&Muestra]) -> Tramo {
    let n = ms.len();
    let bloqueos = ms.iter().filter(|m| m.agachado).count();
    Tramo {
        etiqueta: etiqueta.to_string(),
        n,
        bloqueos,
        br: if n == 0 {
            0.0
        } else {
            bloqueos as f64 / n as f64
        },
    }
}

fn construir_contexto(dimension: &str, tramos: Vec<Tramo>, min_n: usize) -> Contexto {
    let validos: Vec<&Tramo> = tramos.iter().filter(|t| t.n >= min_n).collect();
    let (delta, alto, bajo, p) = if validos.len() >= 2 {
        let mut max = validos[0];
        let mut min = validos[0];
        for t in &validos {
            if t.br > max.br {
                max = t;
            }
            if t.br < min.br {
                min = t;
            }
        }
        (
            Some(max.br - min.br),
            Some(max.etiqueta.clone()),
            Some(min.etiqueta.clone()),
            p_dos_proporciones(max.bloqueos, max.n, min.bloqueos, min.n),
        )
    } else {
        (None, None, None, None)
    };
    let k = validos.len();
    Contexto {
        dimension: dimension.to_string(),
        tramos,
        delta,
        tramo_alto: alto,
        tramo_bajo: bajo,
        p,
        comparaciones: if k >= 2 { k * (k - 1) / 2 } else { 0 },
    }
}

fn en_banda(v: Option<f64>, lo: f64, hi: f64) -> bool {
    matches!(v, Some(x) if x > lo && x <= hi)
}

/// El contexto se analiza solo sobre LOWS: son la decision que interesa.
fn contextos(lows: &[&Muestra], u: &Umbrales) -> Vec<Contexto> {
    let mut out = Vec::new();

    // --- Ronda. Un cheat configurable puede encenderse solo en la decisiva.
    if lows.iter().any(|m| m.ronda.is_some()) {
        let mut por_ronda: BTreeMap<u32, Vec<&Muestra>> = BTreeMap::new();
        for m in lows {
            if let Some(r) = m.ronda {
                por_ronda.entry(r).or_default().push(m);
            }
        }
        let tramos: Vec<Tramo> = por_ronda
            .iter()
            .map(|(r, ms)| tramo(&format!("ronda {r}"), ms))
            .collect();
        out.push(construir_contexto("Ronda", tramos, u.min_n_contexto));
    }

    // --- Marcador antes de la ronda: ir por detras es la condicion tipica.
    if lows
        .iter()
        .any(|m| m.rondas_propias.is_some() && m.rondas_rival.is_some())
    {
        let clasif = |m: &Muestra| -> Option<&'static str> {
            match (m.rondas_propias, m.rondas_rival) {
                (Some(p), Some(r)) if p < r => Some("va por detras"),
                (Some(p), Some(r)) if p > r => Some("va por delante"),
                (Some(_), Some(_)) => Some("empatado"),
                _ => None,
            }
        };
        let mut grupos: BTreeMap<&str, Vec<&Muestra>> = BTreeMap::new();
        for m in lows {
            if let Some(k) = clasif(m) {
                grupos.entry(k).or_default().push(m);
            }
        }
        let tramos: Vec<Tramo> = ["va por detras", "empatado", "va por delante"]
            .iter()
            .filter_map(|k| grupos.get(*k).map(|ms| tramo(k, ms)))
            .collect();
        out.push(construir_contexto("Marcador", tramos, u.min_n_contexto));

        // Punto de partido en contra: el rival puede cerrar el combate.
        let (pp, resto): (Vec<&Muestra>, Vec<&Muestra>) = lows
            .iter()
            .filter(|m| m.rondas_rival.is_some())
            .copied()
            .partition(|m| matches!(m.rondas_rival, Some(r) if r >= 2));
        if !pp.is_empty() && !resto.is_empty() {
            let tramos = vec![
                tramo("punto de partido en contra", &pp),
                tramo("resto", &resto),
            ];
            out.push(construir_contexto(
                "Punto de partido",
                tramos,
                u.min_n_contexto,
            ));
        }
    }

    // --- Vida propia.
    if lows.iter().any(|m| m.vida_pct.is_some()) {
        let bandas: [(&str, f64, f64); 3] = [
            ("vida > 70%", 70.0, 1e9),
            ("vida 30-70%", 30.0, 70.0),
            ("vida <= 30%", -0.1, 30.0),
        ];
        let tramos: Vec<Tramo> = bandas
            .iter()
            .map(|(et, lo, hi)| {
                let ms: Vec<&Muestra> = lows
                    .iter()
                    .filter(|m| en_banda(m.vida_pct, *lo, *hi))
                    .copied()
                    .collect();
                tramo(et, &ms)
            })
            .collect();
        out.push(construir_contexto("Vida propia", tramos, u.min_n_contexto));
    }

    // --- Reloj. Tramos finos: el final de ronda es donde se configura un cheat.
    if lows.iter().any(|m| m.seg_restantes.is_some()) {
        let bandas: [(&str, f64, f64); 4] = [
            ("mas de 40 s", 40.0, 1e9),
            ("40-20 s", 20.0, 40.0),
            ("20-10 s", 10.0, 20.0),
            ("ultimos 10 s", -0.1, 10.0),
        ];
        let tramos: Vec<Tramo> = bandas
            .iter()
            .map(|(et, lo, hi)| {
                let ms: Vec<&Muestra> = lows
                    .iter()
                    .filter(|m| en_banda(m.seg_restantes, *lo, *hi))
                    .copied()
                    .collect();
                tramo(et, &ms)
            })
            .collect();
        out.push(construir_contexto("Reloj", tramos, u.min_n_contexto));
    }

    out
}

// --------------------------------------------------------------- analisis
pub fn analizar(muestras: &[Muestra], sujeto: &str, u: &Umbrales) -> Result<Resultado, Error> {
    if muestras.is_empty() {
        return Err(Error::SinMuestras);
    }
    let mut r = Resultado {
        sujeto: sujeto.to_string(),
        n_muestras: muestras.len(),
        ..Default::default()
    };

    let lows: Vec<&Muestra> = muestras
        .iter()
        .filter(|m| m.altura == Altura::Low)
        .collect();
    let l5050: Vec<&Muestra> = lows
        .iter()
        .filter(|m| m.situacion == Situacion::True5050)
        .copied()
        .collect();
    let ltiming: Vec<&Muestra> = lows
        .iter()
        .filter(|m| m.situacion == Situacion::Timing)
        .copied()
        .collect();
    let m5050: Vec<&Muestra> = muestras
        .iter()
        .filter(|m| m.altura == Altura::Mid && m.situacion == Situacion::True5050)
        .collect();

    // ---- [1] fisica
    //
    // IMPORTANTE: solo se mide en situaciones de TIMING. En un true 50/50 el
    // jugador no reacciona, adivina: se agacha por adelantado, asi que una
    // latencia baja ahi es lo NORMAL en un humano y no prueba nada. Medir la
    // fisica sobre 50/50 acusaria a jugadores limpios.
    let lat: Vec<f64> = ltiming.iter().filter_map(|m| m.latencia_low()).collect();
    r.n_latencias = lat.len();
    r.n_bajo_limite = lat.iter().filter(|x| **x < u.limite_humano).count();
    r.latencia_min = lat
        .iter()
        .fold(None, |a: Option<f64>, x| Some(a.map_or(*x, |v| v.min(*x))));
    r.latencia_mediana = mediana(&lat);
    r.latencia_std = desviacion(&lat);

    // ---- [2] aritmetica del true 50/50
    r.n_5050_lows = l5050.len();
    r.aciertos_5050 = l5050.iter().filter(|m| m.agachado).count();
    if !l5050.is_empty() {
        r.br_5050 = Some(r.aciertos_5050 as f64 / l5050.len() as f64);
        r.p_5050 = binomial_cola_superior(r.aciertos_5050 as u64, l5050.len() as u64);
    }

    // ---- [3] margen hasta el impacto
    let mg: Vec<f64> = lows.iter().filter_map(|m| m.margen()).collect();
    r.n_margenes = mg.len();
    r.margen_medio = media(&mg);
    r.margen_std = desviacion(&mg);
    r.margen_ultimo = mg.iter().filter(|x| **x <= 1.0).count();
    if !mg.is_empty() {
        r.margen_ultimo_frac = Some(r.margen_ultimo as f64 / mg.len() as f64);
    }

    // ---- [4] coherencia del que adivina
    r.n_5050_mids = m5050.len();
    if !m5050.is_empty() {
        let comidos = m5050.iter().filter(|m| m.agachado).count();
        r.br_5050_mids_comidos = Some(comidos as f64 / m5050.len() as f64);
    }
    if let (Some(bl), Some(bm)) = (r.br_5050, r.br_5050_mids_comidos) {
        r.divergencia = Some(bl - bm);
        r.p_divergencia = p_dos_proporciones(
            r.aciertos_5050,
            r.n_5050_lows,
            m5050.iter().filter(|m| m.agachado).count(),
            r.n_5050_mids,
        );
    }

    // ---- timing, referencia interna
    r.n_timing_lows = ltiming.len();
    if !ltiming.is_empty() {
        r.br_timing =
            Some(ltiming.iter().filter(|m| m.agachado).count() as f64 / ltiming.len() as f64);
    }
    r.timing_min = ltiming
        .iter()
        .filter_map(|m| m.latencia_low())
        .fold(None, |a: Option<f64>, x| Some(a.map_or(x, |v| v.min(x))));

    // ---- [5] contexto
    //
    // CORRECCION POR COMPARACIONES MULTIPLES.
    // Se prueban varias dimensiones con varios tramos cada una y se reporta la
    // diferencia MAYOR. Quedarse con el extremo de muchas comparaciones dispara
    // los falsos positivos: es p-hacking. Bonferroni reparte el nivel de
    // significacion entre todas las comparaciones hechas. Es conservador a
    // proposito: aqui acusar a un limpio es peor que no detectar a un tramposo.
    r.contextos = contextos(&lows, u);
    r.n_comparaciones = r.contextos.iter().map(|c| c.comparaciones).sum();
    if r.n_comparaciones > 0 {
        r.p_corregido = Some(u.p_max / r.n_comparaciones as f64);
    }

    // ============================ banderas ============================
    if r.n_bajo_limite > 0 {
        r.banderas.push(format!(
            "POR DEBAJO DEL LIMITE HUMANO: {} de {} bloqueos con latencia menor de \
             {:.0} frames en situaciones de TIMING (la mas rapida, {:.0}f = \
             {:.0} ms). Ahi el jugador afirma estar reaccionando, y por debajo \
             de ese corte no es reaccion: no hay entrenamiento que lo cambie.",
            r.n_bajo_limite,
            r.n_latencias,
            u.limite_humano,
            r.latencia_min.unwrap_or(0.0),
            frames_a_ms(r.latencia_min.unwrap_or(0.0))
        ));
    }

    if let (Some(p), Some(br)) = (r.p_5050, r.br_5050) {
        if r.n_5050_lows >= u.min_n_5050 && p < u.p_max {
            r.banderas.push(format!(
                "SUPERA EL TECHO DEL 50/50: {} aciertos de {} ({:.0}%). Adivinando, \
                 la probabilidad de eso es {:.2e}, o sea 1 entre {:.0}. En un true \
                 50/50 nadie supera el 50% a la larga.",
                r.aciertos_5050,
                r.n_5050_lows,
                br * 100.0,
                p,
                1.0 / p
            ));
        }
    }

    if r.n_margenes >= u.min_n_margen {
        if let Some(sd) = r.margen_std {
            if sd < u.margen_std_min {
                r.banderas.push(format!(
                    "MARGEN CONSTANTE: desviacion tipica de {:.2} frames sobre {} \
                     bloqueos (media {:.1}f). Un humano llega con margen variable; \
                     un programa dispara en el frame exacto. Y un humano no puede \
                     saber cual es el ultimo frame sin conocer el startup y contar.",
                    sd,
                    r.n_margenes,
                    r.margen_medio.unwrap_or(0.0)
                ));
            }
        }
        if let Some(fr) = r.margen_ultimo_frac {
            if fr > u.margen_ultimo_max {
                r.banderas.push(format!(
                    "BLOQUEA EN EL ULTIMO FRAME: {:.0}% de los bloqueos ({} de {}) \
                     llegan con 1 frame o menos de margen. Ese es exactamente el \
                     punto donde dispararia un programa.",
                    fr * 100.0,
                    r.margen_ultimo,
                    r.n_margenes
                ));
            }
        }
    }

    if let (Some(d), Some(bl), Some(bm), Some(pd)) = (
        r.divergencia,
        r.br_5050,
        r.br_5050_mids_comidos,
        r.p_divergencia,
    ) {
        if r.n_5050_lows >= u.min_n_5050
            && r.n_5050_mids >= u.min_n_contexto
            && d > u.divergencia_max
            && pd < u.p_max
        {
            r.banderas.push(format!(
                "INCOHERENTE CON ADIVINAR: bloquea el {:.0}% de los lows pero solo \
                 come el {:.0}% de los mids, {:.0} puntos de diferencia (p={:.4}). \
                 Son la misma decision de agacharse vista por los dos lados: si \
                 adivinara, las dos cifras se parecerian.",
                bl * 100.0,
                bm * 100.0,
                d * 100.0,
                pd
            ));
        }
    }

    // El salto entre tramos tiene que ser grande Y estadisticamente solido.
    // Solo con el salto, dos tramos pequenos se separan por azar y acabariamos
    // acusando a un jugador limpio.
    for c in &r.contextos {
        if let (Some(d), Some(alto), Some(bajo), Some(p)) =
            (c.delta, &c.tramo_alto, &c.tramo_bajo, c.p)
        {
            let corte = r.p_corregido.unwrap_or(u.p_max);
            if d > u.delta_contexto_max && p < corte {
                r.banderas.push(format!(
                    "ACTIVACION SELECTIVA ({}): bloquea {:.0} puntos mas en '{}' que \
                     en '{}' (p={:.5}, corte corregido {:.5}). Los cheats se \
                     configuran por condicion; un \
                     jugador rinde parecido gane o pierda.",
                    c.dimension,
                    d * 100.0,
                    alto,
                    bajo,
                    p,
                    corte
                ));
            }
        }
    }

    // ============================== notas ==============================
    if r.n_5050_lows > 0 && r.n_5050_lows < u.min_n_5050 {
        r.notas.push(format!(
            "Solo {} lows en true 50/50 (minimo {}). Con muestra corta cualquier \
             racha es normal: no se puede concluir nada.",
            r.n_5050_lows, u.min_n_5050
        ));
    }
    if r.n_5050_lows == 0 {
        r.notas.push(
            "Ninguna muestra marcada como true 50/50. Sin ellas se pierde la parte \
             mas solida del analisis: el techo del 50% adivinando."
                .into(),
        );
    }
    if r.n_margenes < u.min_n_margen {
        r.notas.push(format!(
            "Solo {} bloqueos con startup anotado (minimo {}). Rellena la columna \
             'startup' para medir el margen hasta el impacto.",
            r.n_margenes, u.min_n_margen
        ));
    }
    if r.n_5050_mids == 0 {
        r.notas.push(
            "Sin mids anotados en true 50/50. Sin ellos no se ve lo que pierde al \
             equivocarse, que es la comprobacion mas dificil de fingir."
                .into(),
        );
    }
    let faltan_dims: Vec<&str> = ["Ronda", "Marcador", "Vida propia", "Reloj"]
        .into_iter()
        .filter(|d| !r.contextos.iter().any(|c| c.dimension == *d))
        .collect();
    if !faltan_dims.is_empty() {
        r.notas.push(format!(
            "Faltan dimensiones de contexto: {}. Un cheat configurable puede estar \
             atado justo a la que no estas midiendo.",
            faltan_dims.join(", ")
        ));
    }
    if muestras.iter().any(|m| m.online) && muestras.iter().any(|m| !m.online) {
        r.notas.push(
            "AVISO: hay muestras online y offline mezcladas. El rollback desplaza la \
             latencia medida de forma sistematica. Usa --solo-offline o separa los \
             analisis."
                .into(),
        );
    }
    if r.banderas.is_empty() {
        r.notas.push(
            "Ninguna bandera con estos datos y estos umbrales. Eso NO prueba \
             inocencia, igual que una bandera no prueba culpabilidad: prueba que con \
             lo medido no hay desviacion."
                .into(),
        );
    }

    Ok(r)
}

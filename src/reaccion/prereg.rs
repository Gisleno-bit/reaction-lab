//! Pre-registro sellado de umbrales.
//!
//! # Por que existe
//!
//! Si eliges los umbrales DESPUES de ver los datos del sujeto, cualquiera
//! desmonta el analisis en dos frases: elegiste el corte que confirmaba lo que
//! ya creias. No hace falta mala fe — pasa solo, y pasa sobre todo cuando ya
//! sospechas de alguien.
//!
//! El formato es texto plano canonico en vez de JSON a proposito: un humano
//! puede verificarlo a ojo, y la forma canonica es inequivoca, que es lo que
//! importa para que el hash sea reproducible.
//!
//! No es criptografia contra un adversario decidido: quien sella puede sellar
//! varios y publicar el que le convenga. Es disciplina y transparencia —
//! publica el hash en un sitio fechado (un mensaje, un commit) y el sello vale.

use super::nucleo::Umbrales;
use super::sha256;

const CABECERA: &str = "# reaction-lab preregistro v3";

#[derive(Debug, Clone, PartialEq)]
pub struct Preregistro {
    pub sujeto: String,
    pub autor: String,
    pub notas: String,
    pub utc: String,
    pub umbrales: Umbrales,
    pub sha256: String,
}

#[derive(Debug)]
pub enum ErrorPrereg {
    Io(std::io::Error),
    Formato(String),
    SelloInvalido {
        esperado: String,
        recalculado: String,
    },
}

impl std::fmt::Display for ErrorPrereg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorPrereg::Io(e) => write!(f, "error de E/S: {e}"),
            ErrorPrereg::Formato(m) => write!(f, "pre-registro mal formado: {m}"),
            ErrorPrereg::SelloInvalido {
                esperado,
                recalculado,
            } => write!(
                f,
                "SELLO INVALIDO: el pre-registro fue modificado despues de sellarse.\n  \
                 esperado:    {esperado}\n  recalculado: {recalculado}"
            ),
        }
    }
}

impl std::error::Error for ErrorPrereg {}

impl From<std::io::Error> for ErrorPrereg {
    fn from(e: std::io::Error) -> Self {
        ErrorPrereg::Io(e)
    }
}

/// Escapa saltos de linea para que un campo libre no rompa el formato.
fn escapar(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn desescapar(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('\\') => out.push('\\'),
                Some(otro) => out.push(otro),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Cuerpo canonico: lo que entra en el hash, byte a byte.
fn cuerpo(sujeto: &str, autor: &str, notas: &str, utc: &str, u: &Umbrales) -> String {
    format!(
        "{CABECERA}\n\
         sujeto={}\n\
         autor={}\n\
         notas={}\n\
         utc={}\n\
         limite_humano={:.6}\n\
         p_max={:.6}\n\
         margen_std_min={:.6}\n\
         margen_ultimo_max={:.6}\n\
         divergencia_max={:.6}\n\
         delta_contexto_max={:.6}\n\
         min_n_5050={}\n\
         min_n_margen={}\n\
         min_n_contexto={}\n",
        escapar(sujeto),
        escapar(autor),
        escapar(notas),
        utc,
        u.limite_humano,
        u.p_max,
        u.margen_std_min,
        u.margen_ultimo_max,
        u.divergencia_max,
        u.delta_contexto_max,
        u.min_n_5050,
        u.min_n_margen,
        u.min_n_contexto
    )
}

/// Segundos desde la epoca -> "YYYY-MM-DDTHH:MM:SSZ" (algoritmo civil de Howard Hinnant).
pub fn utc_ahora() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let dias = secs.div_euclid(86_400);
    let resto = secs.rem_euclid(86_400);
    let (h, mi, s) = (resto / 3600, (resto % 3600) / 60, resto % 60);

    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, h, mi, s)
}

/// Sella los umbrales y devuelve el documento completo (cuerpo + linea sha256).
pub fn sellar(u: &Umbrales, sujeto: &str, autor: &str, notas: &str) -> (Preregistro, String) {
    let utc = utc_ahora();
    let c = cuerpo(sujeto, autor, notas, &utc, u);
    let digest = sha256::hex(c.as_bytes());
    let doc = format!("{c}sha256={digest}\n");
    (
        Preregistro {
            sujeto: sujeto.to_string(),
            autor: autor.to_string(),
            notas: notas.to_string(),
            utc,
            umbrales: u.clone(),
            sha256: digest,
        },
        doc,
    )
}

/// Parsea y VERIFICA un documento de pre-registro.
pub fn parsear(doc: &str) -> Result<Preregistro, ErrorPrereg> {
    let doc = doc.strip_prefix('\u{feff}').unwrap_or(doc);
    let mut campos = std::collections::BTreeMap::new();
    let mut cuerpo_lineas: Vec<&str> = Vec::new();
    let mut sello: Option<String> = None;

    for linea in doc.lines() {
        let l = linea.trim_end_matches('\r');
        if let Some(resto) = l.strip_prefix("sha256=") {
            sello = Some(resto.trim().to_string());
            break;
        }
        cuerpo_lineas.push(l);
        if l.starts_with('#') || l.is_empty() {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            campos.insert(k.trim().to_string(), v.to_string());
        }
    }

    let sello = sello.ok_or_else(|| ErrorPrereg::Formato("falta la linea sha256=".into()))?;
    let cuerpo_txt = format!("{}\n", cuerpo_lineas.join("\n"));
    let recalculado = sha256::hex(cuerpo_txt.as_bytes());
    if recalculado != sello {
        return Err(ErrorPrereg::SelloInvalido {
            esperado: sello,
            recalculado,
        });
    }

    let num = |k: &str| -> Result<f64, ErrorPrereg> {
        campos
            .get(k)
            .ok_or_else(|| ErrorPrereg::Formato(format!("falta el campo {k}")))?
            .trim()
            .parse::<f64>()
            .map_err(|_| ErrorPrereg::Formato(format!("{k} no es un numero")))
    };
    let ent = |k: &str| -> Result<usize, ErrorPrereg> {
        campos
            .get(k)
            .ok_or_else(|| ErrorPrereg::Formato(format!("falta el campo {k}")))?
            .trim()
            .parse::<usize>()
            .map_err(|_| ErrorPrereg::Formato(format!("{k} no es un entero")))
    };
    let txt = |k: &str| {
        campos
            .get(k)
            .map(|s| desescapar(s.as_str()))
            .unwrap_or_default()
    };

    Ok(Preregistro {
        sujeto: txt("sujeto"),
        autor: txt("autor"),
        notas: txt("notas"),
        utc: txt("utc"),
        umbrales: Umbrales {
            limite_humano: num("limite_humano")?,
            p_max: num("p_max")?,
            margen_std_min: num("margen_std_min")?,
            margen_ultimo_max: num("margen_ultimo_max")?,
            divergencia_max: num("divergencia_max")?,
            delta_contexto_max: num("delta_contexto_max")?,
            min_n_5050: ent("min_n_5050")?,
            min_n_margen: ent("min_n_margen")?,
            min_n_contexto: ent("min_n_contexto")?,
        },
        sha256: sello,
    })
}

pub fn cargar(ruta: &str) -> Result<Preregistro, ErrorPrereg> {
    parsear(&std::fs::read_to_string(ruta)?)
}

impl Preregistro {
    /// Sello corto para imprimir en el informe.
    pub fn sello_corto(&self) -> String {
        format!(
            "{}... ({})",
            &self.sha256[..16.min(self.sha256.len())],
            self.utc
        )
    }
}

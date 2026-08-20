//! Servidor HTTP local para la interfaz grafica.
//!
//! # Por que un servidor local y no una web que calcule sola
//!
//! Toda la estadistica la hace el nucleo de Rust, el mismo que usa la CLI y el
//! que cubren los tests. La interfaz solo recoge datos y pinta el resultado.
//!
//! Si la pagina recalculara medias por su cuenta acabarian existiendo dos
//! implementaciones de lo mismo, y en cuanto se separen un decimal el analisis
//! pierde toda credibilidad: no podrias defender que cifra es la buena.
//!
//! # Seguridad
//!
//! Escucha SOLO en 127.0.0.1. No se expone a la red local ni a internet: los
//! datos de un caso asi no deben salir de la maquina de quien lo analiza.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};

use super::informe;
use super::nucleo::{analizar, parsear_muestras, Filtros, Umbrales};
use super::prereg;

const APP: &str = include_str!("web/app.html");
const MAX_CUERPO: usize = 8 * 1024 * 1024;

struct Peticion {
    metodo: String,
    ruta: String,
    consulta: String,
    cuerpo: String,
}

/// Decodifica %XX y '+' de una query string.
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn param(consulta: &str, clave: &str) -> Option<String> {
    consulta.split('&').find_map(|p| {
        let (k, v) = p.split_once('=')?;
        if url_decode(k) == clave {
            Some(url_decode(v))
        } else {
            None
        }
    })
}

fn leer_peticion(stream: &mut TcpStream) -> std::io::Result<Option<Peticion>> {
    let mut lector = BufReader::new(stream.try_clone()?);
    let mut linea = String::new();
    if lector.read_line(&mut linea)? == 0 {
        return Ok(None);
    }
    let mut partes = linea.split_whitespace();
    let metodo = partes.next().unwrap_or("").to_string();
    let destino = partes.next().unwrap_or("/").to_string();
    let (ruta, consulta) = match destino.split_once('?') {
        Some((r, q)) => (r.to_string(), q.to_string()),
        None => (destino, String::new()),
    };

    let mut largo = 0usize;
    loop {
        let mut cab = String::new();
        if lector.read_line(&mut cab)? == 0 {
            break;
        }
        let t = cab.trim_end();
        if t.is_empty() {
            break;
        }
        if let Some((k, v)) = t.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                largo = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if largo > MAX_CUERPO {
        return Ok(Some(Peticion {
            metodo,
            ruta,
            consulta,
            cuerpo: String::new(),
        }));
    }
    let mut buf = vec![0u8; largo];
    if largo > 0 {
        lector.read_exact(&mut buf)?;
    }
    Ok(Some(Peticion {
        metodo,
        ruta,
        consulta,
        cuerpo: String::from_utf8_lossy(&buf).into_owned(),
    }))
}

fn responder(stream: &mut TcpStream, codigo: u16, tipo: &str, cuerpo: &str) {
    let estado = match codigo {
        200 => "200 OK",
        400 => "400 Bad Request",
        404 => "404 Not Found",
        _ => "500 Internal Server Error",
    };
    let cab = format!(
        "HTTP/1.1 {estado}\r\nContent-Type: {tipo}; charset=utf-8\r\n\
         Content-Length: {}\r\nCache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n",
        cuerpo.len()
    );
    let _ = stream.write_all(cab.as_bytes());
    let _ = stream.write_all(cuerpo.as_bytes());
    let _ = stream.flush();
}

/// Construye los umbrales desde la query string, con los valores por defecto
/// para lo que no venga.
fn umbrales_de_query(q: &str) -> Umbrales {
    let d = Umbrales::default();
    let n = |k: &str, def: f64| param(q, k).and_then(|v| v.parse().ok()).unwrap_or(def);
    let e = |k: &str, def: usize| param(q, k).and_then(|v| v.parse().ok()).unwrap_or(def);
    Umbrales {
        limite_humano: n("limite_humano", d.limite_humano),
        p_max: n("p_max", d.p_max),
        margen_std_min: n("margen_std_min", d.margen_std_min),
        margen_ultimo_max: n("margen_ultimo_max", d.margen_ultimo_max),
        divergencia_max: n("divergencia_max", d.divergencia_max),
        delta_contexto_max: n("delta_contexto_max", d.delta_contexto_max),
        min_n_5050: e("min_n_5050", d.min_n_5050),
        min_n_margen: e("min_n_margen", d.min_n_margen),
        min_n_contexto: e("min_n_contexto", d.min_n_contexto),
    }
}

fn manejar(p: &Peticion) -> (u16, &'static str, String) {
    match (p.metodo.as_str(), p.ruta.as_str()) {
        ("GET", "/") => (200, "text/html", APP.to_string()),

        // Analiza un CSV y devuelve el informe en HTML.
        ("POST", "/api/analizar") => {
            let sujeto = param(&p.consulta, "sujeto").unwrap_or_else(|| "sujeto".into());
            let filtros = Filtros {
                solo_offline: param(&p.consulta, "solo_offline").as_deref() == Some("1"),
                solo_primera: param(&p.consulta, "solo_primera").as_deref() == Some("1"),
                descartar_precrouch: param(&p.consulta, "con_precrouch").as_deref() != Some("1"),
            };

            // Si viene un pre-registro sellado, MANDA sobre lo que diga la
            // interfaz: es justo el punto de tener un sello.
            let (umbrales, sello) = if p.cuerpo.contains("\n--PREREGISTRO--\n") {
                let (csv, doc) = p.cuerpo.split_once("\n--PREREGISTRO--\n").unwrap();
                match prereg::parsear(doc) {
                    Ok(pr) => {
                        let u = pr.umbrales.clone();
                        let s = pr.sello_corto();
                        return analizar_y_responder(csv, &sujeto, &u, Some(&s), filtros);
                    }
                    Err(e) => return (400, "text/plain", format!("{e}")),
                }
            } else {
                (umbrales_de_query(&p.consulta), None)
            };
            analizar_y_responder(&p.cuerpo, &sujeto, &umbrales, sello, filtros)
        }

        // Sella umbrales y devuelve el documento de pre-registro.
        ("POST", "/api/sellar") => {
            let u = umbrales_de_query(&p.consulta);
            let (_, doc) = prereg::sellar(
                &u,
                &param(&p.consulta, "sujeto").unwrap_or_default(),
                &param(&p.consulta, "autor").unwrap_or_default(),
                &param(&p.consulta, "notas").unwrap_or_default(),
            );
            (200, "text/plain", doc)
        }

        // Verifica un sello existente.
        ("POST", "/api/verificar") => match prereg::parsear(&p.cuerpo) {
            Ok(pr) => (
                200,
                "text/plain",
                format!("VALIDO\n{}\n{}\n{}", pr.sujeto, pr.utc, pr.sha256),
            ),
            Err(e) => (400, "text/plain", format!("{e}")),
        },

        ("GET", "/api/plantilla") => (200, "text/plain", super::datos::PLANTILLA.to_string()),

        ("GET", "/api/demo") => {
            let perfil = param(&p.consulta, "perfil").unwrap_or_else(|| "script".into());
            match super::datos::Perfil::desde_str(&perfil) {
                Some(pf) => (200, "text/plain", super::datos::generar_demo(pf, 5, 3)),
                None => (400, "text/plain", "perfil desconocido".into()),
            }
        }

        _ => (404, "text/plain", "no encontrado".into()),
    }
}

fn analizar_y_responder(
    csv: &str,
    sujeto: &str,
    u: &Umbrales,
    sello: Option<&str>,
    filtros: Filtros,
) -> (u16, &'static str, String) {
    let (ms, d) = match parsear_muestras(csv, filtros) {
        Ok(v) => v,
        Err(e) => return (400, "text/plain", format!("{e}")),
    };
    match analizar(&ms, sujeto, u) {
        Ok(r) => {
            let mut html = informe::html(&r, sello);
            if d.hay() {
                html = html.replace(
                    "<h2>1 · Limite fisico</h2>",
                    &format!(
                        "<div class=\"nota\">Filas descartadas por los filtros: {}</div>\
                         <h2>1 · Limite fisico</h2>",
                        d.resumen()
                    ),
                );
            }
            (200, "text/html", html)
        }
        Err(e) => (400, "text/plain", format!("{e}")),
    }
}

/// Arranca el servidor en 127.0.0.1:puerto y bloquea hasta Ctrl+C.
pub fn arrancar(puerto: u16) -> std::io::Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, puerto))?;
    let real = listener.local_addr()?.port();
    println!("Reaction Lab escuchando en http://127.0.0.1:{real}");
    println!("Abre esa direccion en el navegador. Ctrl+C para parar.\n");
    println!("Solo escucha en local: nada sale de esta maquina.");

    for conexion in listener.incoming() {
        let mut stream = match conexion {
            Ok(s) => s,
            Err(_) => continue,
        };
        std::thread::spawn(move || {
            match leer_peticion(&mut stream) {
                Ok(Some(p)) => {
                    let (codigo, tipo, cuerpo) = manejar(&p);
                    responder(&mut stream, codigo, tipo, &cuerpo);
                }
                _ => responder(&mut stream, 400, "text/plain", "peticion invalida"),
            };
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodifica_url() {
        assert_eq!(url_decode("hola+mundo"), "hola mundo");
        assert_eq!(url_decode("a%20b"), "a b");
        assert_eq!(url_decode("caf%C3%A9"), "café");
        assert_eq!(url_decode("sin_cambios"), "sin_cambios");
        assert_eq!(url_decode("%ZZ"), "%ZZ");
    }

    #[test]
    fn extrae_parametros() {
        let q = "sujeto=kai&solo_offline=1&autor=Tekken+Espa%C3%B1a";
        assert_eq!(param(q, "sujeto").as_deref(), Some("kai"));
        assert_eq!(param(q, "solo_offline").as_deref(), Some("1"));
        assert_eq!(param(q, "autor").as_deref(), Some("Tekken España"));
        assert_eq!(param(q, "nope"), None);
    }

    #[test]
    fn umbrales_por_defecto_si_no_vienen() {
        let u = umbrales_de_query("");
        assert_eq!(u, Umbrales::default());
    }

    #[test]
    fn umbrales_de_query_sobrescriben() {
        let u = umbrales_de_query("limite_humano=18&min_n_5050=20");
        assert_eq!(u.limite_humano, 18.0);
        assert_eq!(u.min_n_5050, 20);
        assert_eq!(u.p_max, Umbrales::default().p_max);
    }

    #[test]
    fn ruta_desconocida_da_404() {
        let p = Peticion {
            metodo: "GET".into(),
            ruta: "/otra".into(),
            consulta: String::new(),
            cuerpo: String::new(),
        };
        assert_eq!(manejar(&p).0, 404);
    }

    #[test]
    fn la_raiz_sirve_la_app() {
        let p = Peticion {
            metodo: "GET".into(),
            ruta: "/".into(),
            consulta: String::new(),
            cuerpo: String::new(),
        };
        let (c, t, cuerpo) = manejar(&p);
        assert_eq!((c, t), (200, "text/html"));
        assert!(cuerpo.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn analizar_csv_invalido_da_400() {
        let p = Peticion {
            metodo: "POST".into(),
            ruta: "/api/analizar".into(),
            consulta: "sujeto=kai".into(),
            cuerpo: "a,b,c\n1,2,3\n".into(),
        };
        assert_eq!(manejar(&p).0, 400);
    }

    #[test]
    fn analizar_csv_valido_devuelve_informe() {
        let csv = super::super::datos::generar_demo(super::super::datos::Perfil::Script, 5, 3);
        let p = Peticion {
            metodo: "POST".into(),
            ruta: "/api/analizar".into(),
            consulta: "sujeto=kai".into(),
            cuerpo: csv,
        };
        let (c, t, cuerpo) = manejar(&p);
        assert_eq!((c, t), (200, "text/html"));
        assert!(cuerpo.contains("TECHO DEL 50/50"));
    }

    #[test]
    fn el_preregistro_manda_sobre_la_interfaz() {
        // Umbrales imposibles en el sello: aunque la query pida los normales,
        // no debe salir ninguna bandera.
        let imposibles = Umbrales {
            p_max: 0.0,
            margen_std_min: 0.0,
            margen_ultimo_max: 1.1,
            divergencia_max: 9.9,
            delta_contexto_max: 9.9,
            ..Default::default()
        };
        let (_, doc) = prereg::sellar(&imposibles, "kai", "org", "");
        let csv = super::super::datos::generar_demo(super::super::datos::Perfil::Script, 5, 3);
        let p = Peticion {
            metodo: "POST".into(),
            ruta: "/api/analizar".into(),
            consulta: "sujeto=kai&p_max=0.01".into(),
            cuerpo: format!("{csv}\n--PREREGISTRO--\n{doc}"),
        };
        let (c, _, cuerpo) = manejar(&p);
        assert_eq!(c, 200);
        assert!(
            cuerpo.contains("Ninguna bandera"),
            "el sello debe mandar sobre los umbrales de la interfaz"
        );
    }

    #[test]
    fn sellar_y_verificar_por_api() {
        let p = Peticion {
            metodo: "POST".into(),
            ruta: "/api/sellar".into(),
            consulta: "sujeto=kai&autor=org&limite_humano=21".into(),
            cuerpo: String::new(),
        };
        let (c, _, doc) = manejar(&p);
        assert_eq!(c, 200);
        let v = Peticion {
            metodo: "POST".into(),
            ruta: "/api/verificar".into(),
            consulta: String::new(),
            cuerpo: doc.clone(),
        };
        assert_eq!(manejar(&v).0, 200);

        let manipulado = doc.replace("limite_humano=21.000000", "limite_humano=12.000000");
        assert_ne!(doc, manipulado);
        let m = Peticion {
            metodo: "POST".into(),
            ruta: "/api/verificar".into(),
            consulta: String::new(),
            cuerpo: manipulado,
        };
        assert_eq!(manejar(&m).0, 400, "un sello manipulado debe rechazarse");
    }
}

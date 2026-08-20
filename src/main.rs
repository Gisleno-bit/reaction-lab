//! CLI de Reaction Lab.

use std::process::ExitCode;

use reaction_lab::reaccion::datos::{generar_demo, Perfil, PLANTILLA};
use reaction_lab::reaccion::informe;
use reaction_lab::reaccion::nucleo::{analizar, cargar_csv, parsear_muestras, Filtros, Umbrales};
use reaction_lab::reaccion::prereg;
use reaction_lab::reaccion::servidor;
use reaction_lab::VERSION;

const AYUDA: &str = "\
reaction-lab — analisis de reacciones a lows sin necesidad de jugador de control

Se apoya en dos cosas que no se pueden rebatir: la FISICA (por debajo del limite
humano no es reaccion) y la ARITMETICA (en un true 50/50 hay que adivinar, y el
techo adivinando es 50%). Ademas mide el margen hasta el impacto y el contexto,
porque los cheats se configuran por condicion.

USO
  reaction-lab <comando> [opciones]

COMANDOS
  interfaz             abre la interfaz grafica en el navegador
  plantilla            escribe el CSV de plantilla
  demo                 genera datos simulados y los analiza
  sellar               sella los umbrales ANTES de mirar los datos
  verificar            comprueba el sello de un pre-registro
  analizar             corre el analisis sobre un CSV

OPCIONES DE 'interfaz'
  --puerto <n>         puerto local (por defecto 8787; 0 = uno libre)

OPCIONES DE 'analizar'
  --csv <ruta>         CSV de muestras                      (obligatorio)
  --sujeto <nombre>    nombre para el informe               (obligatorio)
  --preregistro <ruta> pre-registro sellado                 (recomendado)
  --solo-offline       descarta filas online                (recomendado)
  --solo-primera       solo nth=1: separa reaccion de anticipacion
  --con-precrouch      NO descartar filas con precrouch=1
  -o <ruta>            guarda el informe (.txt o .html segun extension)

OPCIONES DE 'sellar'
  --sujeto <nombre>    (obligatorio)   --autor <texto>   --notas <texto>
  --limite-humano <f>       frames por debajo de los cuales no es reaccion (21)
  --p-max <f>               significacion exigida al binomial (0.01)
  --margen-std <f>          std minima del margen que se considera humana (1.0)
  --margen-ultimo <f>       fraccion maxima de bloqueos en el ultimo frame (0.6)
  --divergencia <f>         divergencia maxima lows/mids en 50/50 (0.25)
  --delta-contexto <f>      salto maximo entre tramos de contexto (0.25)
  --min-n-5050 <n>          (8)   --min-n-margen <n> (6)   --min-n-contexto <n> (12)
  -o <ruta>                 (preregistro.txt)

OPCIONES DE 'demo'
  --perfil <p>         humano | script | selectivo   (por defecto: script)
  --rondas <n>         rondas simuladas              (por defecto: 3)
  -o <ruta>            guarda tambien el CSV generado

EJEMPLO
  reaction-lab sellar --sujeto kai --autor \"Tekken Espana\" -o pr.txt
  reaction-lab plantilla -o muestras.csv
  reaction-lab analizar --csv muestras.csv --sujeto kai \\
      --preregistro pr.txt --solo-offline -o informe.html

Mide desviaciones estadisticas y limites fisicos. No inspecciona ninguna maquina.
";

struct Args {
    libres: Vec<String>,
    banderas: std::collections::BTreeSet<String>,
    valores: std::collections::BTreeMap<String, String>,
}

impl Args {
    fn parsear(argv: &[String], con_valor: &[&str]) -> Result<Args, String> {
        let mut a = Args {
            libres: Vec::new(),
            banderas: Default::default(),
            valores: Default::default(),
        };
        let mut i = 0;
        while i < argv.len() {
            let t = &argv[i];
            if let Some(nombre) = t.strip_prefix("--").or_else(|| t.strip_prefix('-')) {
                let (clave, inline) = match nombre.split_once('=') {
                    Some((k, v)) => (k.to_string(), Some(v.to_string())),
                    None => (nombre.to_string(), None),
                };
                if con_valor.contains(&clave.as_str()) {
                    let v = match inline {
                        Some(v) => v,
                        None => {
                            i += 1;
                            argv.get(i)
                                .cloned()
                                .ok_or_else(|| format!("la opcion --{clave} necesita un valor"))?
                        }
                    };
                    a.valores.insert(clave, v);
                } else {
                    if inline.is_some() {
                        return Err(format!("la opcion --{clave} no acepta valor"));
                    }
                    a.banderas.insert(clave);
                }
            } else {
                a.libres.push(t.clone());
            }
            i += 1;
        }
        Ok(a)
    }

    fn tiene(&self, k: &str) -> bool {
        self.banderas.contains(k)
    }
    fn val(&self, k: &str) -> Option<&str> {
        self.valores.get(k).map(|s| s.as_str())
    }
    fn num(&self, k: &str, def: f64) -> Result<f64, String> {
        match self.val(k) {
            None => Ok(def),
            Some(v) => v
                .parse()
                .map_err(|_| format!("--{k} debe ser un numero, no '{v}'")),
        }
    }
    fn ent(&self, k: &str, def: usize) -> Result<usize, String> {
        match self.val(k) {
            None => Ok(def),
            Some(v) => v
                .parse()
                .map_err(|_| format!("--{k} debe ser un entero, no '{v}'")),
        }
    }
}

fn escribir(ruta: &str, contenido: &str) -> Result<(), String> {
    std::fs::write(ruta, contenido).map_err(|e| format!("no pude escribir {ruta}: {e}"))
}

fn umbrales_de(a: &Args) -> Result<Umbrales, String> {
    Ok(Umbrales {
        limite_humano: a.num("limite-humano", 21.0)?,
        p_max: a.num("p-max", 0.01)?,
        margen_std_min: a.num("margen-std", 1.0)?,
        margen_ultimo_max: a.num("margen-ultimo", 0.60)?,
        divergencia_max: a.num("divergencia", 0.25)?,
        delta_contexto_max: a.num("delta-contexto", 0.25)?,
        min_n_5050: a.ent("min-n-5050", 8)?,
        min_n_margen: a.ent("min-n-margen", 6)?,
        min_n_contexto: a.ent("min-n-contexto", 12)?,
    })
}

fn cmd_interfaz(a: &Args) -> Result<(), String> {
    let puerto = a.ent("puerto", 8787)?;
    if puerto > 65535 {
        return Err("--puerto debe estar entre 0 y 65535".into());
    }
    servidor::arrancar(puerto as u16)
        .map_err(|e| format!("no pude abrir el puerto {puerto}: {e}. Prueba con --puerto 0."))
}

fn cmd_plantilla(a: &Args) -> Result<(), String> {
    let ruta = a.val("o").unwrap_or("muestras.csv");
    escribir(ruta, PLANTILLA)?;
    println!("Escrito {ruta}\n");
    println!("Una fila POR CADA GOLPE lanzado en la situacion, lows y mids:");
    println!("  situacion       true5050 (hay que adivinar) | timing (se puede reaccionar)");
    println!("  altura          low | mid");
    println!("  startup         frame en que el move golpea. Sin esto no hay margen.");
    println!("  latency         frames desde el FRAME 1 de la animacion al input de");
    println!("                  guardia baja. Vacio si no se agacho.");
    println!("  agachado        1/0. En un low = bloqueo; en un mid = se lo comio.");
    println!("  vida_pct        vida del defensor, 0-100");
    println!("  ronda           1, 2, 3...");
    println!("  rondas_propias  rondas ganadas por cada lado ANTES de esta ronda");
    println!("  rondas_rival");
    println!("  seg_restantes   reloj tal y como se ve en pantalla");
    println!("  online          1/0. NUNCA mezcles online y offline.");
    println!("\nRellena el contexto en TODAS las filas, no solo en las sospechosas:");
    println!("si solo anotas los momentos raros, encontraras el patron que buscabas");
    println!("aunque no exista.");
    Ok(())
}

fn cmd_demo(a: &Args) -> Result<(), String> {
    let perfil = Perfil::desde_str(a.val("perfil").unwrap_or("script"))
        .ok_or("--perfil debe ser humano, script o selectivo")?;
    let rondas = a.ent("rondas", 3)? as u32;
    if rondas == 0 || rondas > 9 {
        return Err("--rondas debe estar entre 1 y 9".into());
    }
    let csv = generar_demo(perfil, 5, rondas);
    if let Some(r) = a.val("o") {
        escribir(r, &csv)?;
        println!("CSV generado en {r}\n");
    }
    println!(
        "MODO DEMO — datos simulados, perfil '{}'.\n",
        perfil.etiqueta()
    );
    let f = Filtros {
        descartar_precrouch: true,
        ..Default::default()
    };
    let (ms, _) = parsear_muestras(&csv, f).map_err(|e| e.to_string())?;
    let r = analizar(&ms, "kai", &Umbrales::default()).map_err(|e| e.to_string())?;
    println!("{}", informe::texto(&r, None));
    Ok(())
}

fn cmd_sellar(a: &Args) -> Result<(), String> {
    let sujeto = a.val("sujeto").ok_or("falta --sujeto")?;
    let u = umbrales_de(a)?;
    let (pr, doc) = prereg::sellar(
        &u,
        sujeto,
        a.val("autor").unwrap_or(""),
        a.val("notas").unwrap_or(""),
    );
    let ruta = a.val("o").unwrap_or("preregistro.txt");
    escribir(ruta, &doc)?;
    println!("Pre-registro sellado en {ruta}");
    println!("SHA-256: {}", pr.sha256);
    println!("UTC:     {}", pr.utc);
    println!("\nPublica ESE hash ahora, antes de mirar los datos del sujeto.");
    println!("Cuando salga el informe, cualquiera podra recalcularlo y comprobar");
    println!("que los umbrales no se movieron por el camino.");
    Ok(())
}

fn cmd_verificar(a: &Args) -> Result<(), String> {
    let ruta = a
        .val("preregistro")
        .or_else(|| a.libres.first().map(|s| s.as_str()))
        .ok_or("indica la ruta del pre-registro")?;
    let pr = prereg::cargar(ruta).map_err(|e| e.to_string())?;
    println!("SELLO VALIDO");
    println!("  sujeto:  {}", pr.sujeto);
    println!("  autor:   {}", pr.autor);
    println!("  utc:     {}", pr.utc);
    println!("  sha256:  {}", pr.sha256);
    println!("  umbrales: {:?}", pr.umbrales);
    Ok(())
}

fn cmd_analizar(a: &Args) -> Result<(), String> {
    let csv = a.val("csv").ok_or("falta --csv")?;
    let sujeto = a.val("sujeto").ok_or("falta --sujeto")?;

    let (umbrales, sello) = match a.val("preregistro") {
        Some(p) => {
            let pr = prereg::cargar(p).map_err(|e| e.to_string())?;
            (pr.umbrales.clone(), Some(pr.sello_corto()))
        }
        None => {
            eprintln!(
                "AVISO: sin pre-registro, umbrales por defecto. Para un informe\n\
                 publicable sella primero:  reaction-lab sellar --sujeto {sujeto}\n"
            );
            (Umbrales::default(), None)
        }
    };

    let filtros = Filtros {
        solo_offline: a.tiene("solo-offline"),
        solo_primera: a.tiene("solo-primera"),
        descartar_precrouch: !a.tiene("con-precrouch"),
    };
    let (ms, d) = cargar_csv(csv, filtros).map_err(|e| e.to_string())?;
    if d.hay() {
        println!("Filas descartadas: {}\n", d.resumen());
    }
    let r = analizar(&ms, sujeto, &umbrales).map_err(|e| e.to_string())?;

    let txt = informe::texto(&r, sello.as_deref());
    println!("{txt}");

    if let Some(salida) = a.val("o") {
        let contenido = if salida.to_lowercase().ends_with(".html") {
            informe::html(&r, sello.as_deref())
        } else {
            txt
        };
        escribir(salida, &contenido)?;
        println!("[informe guardado en {salida}]");
    }
    Ok(())
}

fn ejecutar() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv[0] == "--help" || argv[0] == "-h" || argv[0] == "help" {
        print!("{AYUDA}");
        return Ok(());
    }
    if argv[0] == "--version" || argv[0] == "-V" {
        println!("reaction-lab {VERSION}");
        return Ok(());
    }

    let con_valor = [
        "csv",
        "sujeto",
        "preregistro",
        "o",
        "perfil",
        "rondas",
        "autor",
        "notas",
        "limite-humano",
        "p-max",
        "margen-std",
        "margen-ultimo",
        "divergencia",
        "delta-contexto",
        "min-n-5050",
        "min-n-margen",
        "min-n-contexto",
        "puerto",
    ];
    let a = Args::parsear(&argv[1..], &con_valor)?;

    match argv[0].as_str() {
        "interfaz" => cmd_interfaz(&a),
        "plantilla" => cmd_plantilla(&a),
        "demo" => cmd_demo(&a),
        "sellar" => cmd_sellar(&a),
        "verificar" => cmd_verificar(&a),
        "analizar" => cmd_analizar(&a),
        otro => Err(format!("comando desconocido: '{otro}'. Usa --help.")),
    }
}

fn main() -> ExitCode {
    match ejecutar() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

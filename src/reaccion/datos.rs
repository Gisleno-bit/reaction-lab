//! Plantilla CSV y generador de datos simulados para demo y tests.

use super::estadistica::Rng;

pub const CABECERA: &str = "situacion,move_id,altura,startup,latency,agachado,\
vida_pct,ronda,rondas_propias,rondas_rival,seg_restantes,online,precrouch,nth";

pub const PLANTILLA: &str = "\
situacion,move_id,altura,startup,latency,agachado,vida_pct,ronda,rondas_propias,rondas_rival,seg_restantes,online,precrouch,nth
true5050,hellsweep,low,18,,0,82,1,0,0,54,0,0,1
true5050,df2,mid,,,0,82,1,0,0,52,0,0,1
true5050,hellsweep,low,18,16,1,61,1,0,0,38,0,0,2
timing,db4,low,24,23,1,61,1,0,0,31,0,0,1
timing,db4,low,24,,0,45,2,1,0,49,0,0,2
true5050,d4,low,19,17,1,28,3,1,1,12,0,0,1
true5050,uf4,mid,,,1,28,3,1,1,9,0,0,1
";

/// Firma simulada del defensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Perfil {
    /// Adivina en los 50/50 y reacciona por encima del limite en timing.
    Humano,
    /// Auto-block: bloquea en `startup - 1` siempre, gane o pierda.
    Script,
    /// Cheat configurado: solo se enciende en ronda 3 y con poca vida.
    Selectivo,
}

impl Perfil {
    pub fn desde_str(s: &str) -> Option<Perfil> {
        match s.trim().to_lowercase().as_str() {
            "humano" => Some(Perfil::Humano),
            "script" => Some(Perfil::Script),
            "selectivo" => Some(Perfil::Selectivo),
            _ => None,
        }
    }
    pub fn etiqueta(self) -> &'static str {
        match self {
            Perfil::Humano => "humano",
            Perfil::Script => "script",
            Perfil::Selectivo => "selectivo",
        }
    }
}

const LOWS: [(&str, u32); 4] = [("hellsweep", 18), ("db4", 18), ("d4", 19), ("FC df4", 20)];
const MIDS: [&str; 4] = ["df2", "b3", "uf4", "df1"];

fn uni(rng: &mut Rng) -> f64 {
    (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

fn gauss(rng: &mut Rng, mu: f64, sigma: f64) -> f64 {
    let u1 = uni(rng).max(f64::MIN_POSITIVE);
    let u2 = uni(rng);
    mu + sigma * (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Genera un CSV simulado. Determinista para una semilla dada.
pub fn generar_demo(perfil: Perfil, semilla: u64, rondas: u32) -> String {
    let mut rng = Rng::new(semilla);
    let mut s = String::from(CABECERA);
    s.push('\n');

    for ronda in 1..=rondas {
        // Marcador antes de esta ronda.
        let (rp, rr) = match ronda {
            1 => (0, 0),
            2 => (0, 1),
            _ => (1, 1),
        };
        let critica = ronda == rondas;

        for k in 0..30 {
            let (mv, su) = LOWS[k % 4];
            let vida = if critica {
                8.0 + uni(&mut rng) * 24.0
            } else {
                40.0 + uni(&mut rng) * 55.0
            };
            let seg = 58.0 - k as f64 * 1.8;

            let activo = match perfil {
                Perfil::Humano => false,
                Perfil::Script => true,
                Perfil::Selectivo => critica && vida <= 30.0,
            };

            // --- true 50/50: hay que adivinar
            let (ag, lat) = if activo {
                (true, Some(su as f64 - 1.0))
            } else if uni(&mut rng) < 0.5 {
                (true, Some(gauss(&mut rng, su as f64 - 4.0, 2.6).max(1.0)))
            } else {
                (false, None)
            };
            s.push_str(&format!(
                "true5050,{},low,{},{},{},{:.0},{},{},{},{:.0},0,0,{}\n",
                mv,
                su,
                lat.map(|v| format!("{:.0}", v)).unwrap_or_default(),
                if ag { 1 } else { 0 },
                vida,
                ronda,
                rp,
                rr,
                seg.max(1.0),
                k % 3 + 1
            ));

            // --- el mid del mismo 50/50: agacharse aqui es comerselo
            if k % 2 == 0 {
                let comido = if activo {
                    uni(&mut rng) < 0.05
                } else {
                    uni(&mut rng) < 0.5
                };
                s.push_str(&format!(
                    "true5050,{},mid,,,{},{:.0},{},{},{},{:.0},0,0,1\n",
                    MIDS[k % 4],
                    if comido { 1 } else { 0 },
                    vida,
                    ronda,
                    rp,
                    rr,
                    (seg - 1.0).max(1.0)
                ));
            }
        }

        // --- situaciones de timing: se puede reaccionar de verdad
        for k in 0..16 {
            let (mv, _) = LOWS[k % 4];
            let su = 24 + (k as u32 % 4);
            let vida = 35.0 + uni(&mut rng) * 60.0;
            let seg = 50.0 - k as f64 * 2.8;
            // Un auto-block tambien dispara aqui: bloquea todo, siempre en
            // el mismo frame. Un humano reacciona por encima del limite y con
            // margen variable.
            let vida_baja = vida <= 30.0;
            let activo = match perfil {
                Perfil::Humano => false,
                Perfil::Script => true,
                Perfil::Selectivo => ronda == rondas && vida_baja,
            };
            let bloquea = if activo { true } else { uni(&mut rng) < 0.72 };
            let lat = if activo {
                Some(su as f64 - 1.0)
            } else if bloquea {
                Some(gauss(&mut rng, 23.0, 1.9).max(21.0))
            } else {
                None
            };
            s.push_str(&format!(
                "timing,{},low,{},{},{},{:.0},{},{},{},{:.0},0,0,{}\n",
                mv,
                su,
                lat.map(|v| format!("{:.0}", v)).unwrap_or_default(),
                if bloquea { 1 } else { 0 },
                vida,
                ronda,
                rp,
                rr,
                seg.max(1.0),
                k % 3 + 1
            ));
        }
    }
    s
}

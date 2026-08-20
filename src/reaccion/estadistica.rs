//! Primitivas estadisticas. Todo a mano, sin dependencias, para que el calculo
//! sea auditable por cualquiera que lea el repo.

/// Generador xorshift64* — determinista, semilla explicita.
/// El bootstrap debe ser reproducible: mismo CSV, mismo intervalo.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(semilla: u64) -> Self {
        Rng(if semilla == 0 {
            0x9E3779B97F4A7C15
        } else {
            semilla
        })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Entero uniforme en [0, n) sin sesgo de modulo.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "below(0) no tiene sentido");
        let n64 = n as u64;
        let limite = u64::MAX - (u64::MAX % n64);
        loop {
            let v = self.next_u64();
            if v < limite {
                return (v % n64) as usize;
            }
        }
    }
}

pub fn media(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    Some(xs.iter().sum::<f64>() / xs.len() as f64)
}

pub fn mediana(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

/// Desviacion tipica muestral (denominador n-1). None con menos de 2 datos.
pub fn desviacion(xs: &[f64]) -> Option<f64> {
    if xs.len() < 2 {
        return None;
    }
    let m = media(xs)?;
    let suma: f64 = xs.iter().map(|x| (x - m) * (x - m)).sum();
    Some((suma / (xs.len() as f64 - 1.0)).sqrt())
}

/// Intervalo de confianza por bootstrap percentil.
/// `estimador` se aplica a cada remuestreo. Semilla fija = resultado reproducible.
pub fn bootstrap_ci<F>(
    xs: &[f64],
    estimador: F,
    iteraciones: usize,
    alfa: f64,
    semilla: u64,
) -> Option<(f64, f64)>
where
    F: Fn(&[f64]) -> Option<f64>,
{
    if xs.len() < 3 {
        return None;
    }
    let mut rng = Rng::new(semilla);
    let mut vals = Vec::with_capacity(iteraciones);
    let mut buf = vec![0.0; xs.len()];
    for _ in 0..iteraciones {
        for slot in buf.iter_mut() {
            *slot = xs[rng.below(xs.len())];
        }
        if let Some(v) = estimador(&buf) {
            vals.push(v);
        }
    }
    if vals.len() < 10 {
        return None;
    }
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo_i = ((alfa / 2.0) * vals.len() as f64) as usize;
    let hi_i = (((1.0 - alfa / 2.0) * vals.len() as f64) as usize).min(vals.len() - 1);
    Some((vals[lo_i], vals[hi_i]))
}

/// ln(n!) sumando logaritmos. Exacto de sobra para los tamanos de muestra
/// reales y trivialmente verificable, a diferencia de una aproximacion Lanczos.
fn ln_factorial(n: u64) -> f64 {
    let mut s = 0.0f64;
    for k in 2..=n {
        s += (k as f64).ln();
    }
    s
}

fn ln_binomial(n: u64, k: u64) -> f64 {
    ln_factorial(n) - ln_factorial(k) - ln_factorial(n - k)
}

/// Probabilidad exacta de obtener AL MENOS `k` aciertos en `n` intentos
/// lanzando una moneda. Es el numero que cierra un true 50/50: si hay que
/// adivinar, el techo es 50% y esto dice cuanto se sale de ahi.
pub fn binomial_cola_superior(k: u64, n: u64) -> Option<f64> {
    if n == 0 || k > n {
        return None;
    }
    let ln2 = std::f64::consts::LN_2;
    let mut acum = 0.0f64;
    for i in k..=n {
        acum += (ln_binomial(n, i) - (n as f64) * ln2).exp();
    }
    Some(acum.min(1.0))
}

/// Funcion error, aproximacion de Abramowitz y Stegun 7.1.26.
/// Error absoluto < 1.5e-7, de sobra para lo que se usa aqui.
fn erf(x: f64) -> f64 {
    let signo = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let y = 1.0
        - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t
            + 0.254829592)
            * t
            * (-x * x).exp();
    signo * y
}

/// Funcion de distribucion acumulada de la normal estandar.
pub fn phi(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

/// p bilateral del test z de dos proporciones.
///
/// Sirve para no confundir ruido con senal: con muestras pequenas, dos tramos
/// de contexto pueden separarse 30 puntos por puro azar. Sin esto, la
/// herramienta acusaria a jugadores limpios, que es su peor fallo posible.
pub fn p_dos_proporciones(exitos1: usize, n1: usize, exitos2: usize, n2: usize) -> Option<f64> {
    if n1 < 2 || n2 < 2 {
        return None;
    }
    let (p1, p2) = (exitos1 as f64 / n1 as f64, exitos2 as f64 / n2 as f64);
    let pool = (exitos1 + exitos2) as f64 / (n1 + n2) as f64;
    let se = (pool * (1.0 - pool) * (1.0 / n1 as f64 + 1.0 / n2 as f64)).sqrt();
    if se <= 0.0 {
        return Some(1.0);
    }
    let z = (p1 - p2) / se;
    Some((2.0 * (1.0 - phi(z.abs()))).clamp(0.0, 1.0))
}

/// Resultado del test de signos.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TestSignos {
    /// p bilateral. `None` si la muestra es demasiado corta (< 3 no nulos).
    pub p: Option<f64>,
    /// Cuantos valores caen por debajo de cero.
    pub bajo_cero: u64,
    /// Total de valores no nulos considerados.
    pub n: u64,
}

/// Test de signos exacto (binomial) contra la hipotesis nula de mediana 0.
///
/// # Por que este y no un test de permutacion por signos
///
/// Un test de permutacion por cambio de signo **no puede rechazar** cuando los
/// datos tienen dispersion muy baja: con valores casi identicos, la mediana
/// permutada iguala siempre a la observada y p tiende a 1. Eso es exactamente
/// la firma de un auto-block — la mas sospechosa de todas seria la que menos
/// se detectara. El test de signos no tiene ese punto ciego.
///
/// Ademas se explica solo: "de 34 reacciones medidas, 34 fueron mas rapidas
/// que la media del baseline; la probabilidad de eso a cara o cruz es X".
pub fn test_signos(xs: &[f64]) -> TestSignos {
    let no_nulos: Vec<f64> = xs.iter().copied().filter(|x| *x != 0.0).collect();
    let n = no_nulos.len() as u64;
    let bajo = no_nulos.iter().filter(|x| **x < 0.0).count() as u64;
    if n < 3 {
        return TestSignos {
            p: None,
            bajo_cero: bajo,
            n,
        };
    }
    let cola = bajo.min(n - bajo);
    let ln2 = std::f64::consts::LN_2;
    let mut acum = 0.0f64;
    for i in 0..=cola {
        acum += (ln_binomial(n, i) - (n as f64) * ln2).exp();
    }
    TestSignos {
        p: Some((2.0 * acum).min(1.0)),
        bajo_cero: bajo,
        n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mediana_par_e_impar() {
        assert_eq!(mediana(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(mediana(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(mediana(&[]), None);
    }

    #[test]
    fn desviacion_muestral() {
        let d = desviacion(&[2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0]).unwrap();
        assert!((d - 2.13809).abs() < 1e-4, "salio {d}");
        assert_eq!(desviacion(&[1.0]), None);
    }

    #[test]
    fn signos_detecta_sin_dispersion() {
        // Regresion: un test de permutacion fallaba justo aqui, que es la
        // firma MAS sospechosa (valores identicos y todos negativos).
        let t = test_signos(&[-3.0; 20]);
        assert_eq!((t.bajo_cero, t.n), (20, 20));
        let p = t.p.unwrap();
        assert!(p < 1e-5, "p salio {p}");
        // 2 * 0.5^20 exacto
        assert!((p - 2.0 * 0.5f64.powi(20)).abs() < 1e-12);
    }

    #[test]
    fn signos_no_detecta_equilibrio() {
        let xs: Vec<f64> = (0..40)
            .map(|i| if i % 2 == 0 { 1.0 } else { -1.0 })
            .collect();
        assert!(test_signos(&xs).p.unwrap() > 0.5);
    }

    #[test]
    fn signos_muestra_corta() {
        assert!(test_signos(&[-1.0, -2.0]).p.is_none());
    }

    #[test]
    fn signos_ignora_ceros() {
        let t = test_signos(&[0.0, 0.0, -1.0, -1.0, -1.0]);
        assert_eq!(t.n, 3);
        assert_eq!(t.bajo_cero, 3);
    }

    #[test]
    fn bootstrap_es_reproducible() {
        let xs: Vec<f64> = (0..40).map(|i| i as f64).collect();
        let a = bootstrap_ci(&xs, media, 2000, 0.05, 11).unwrap();
        let b = bootstrap_ci(&xs, media, 2000, 0.05, 11).unwrap();
        assert_eq!(a, b, "mismo CSV debe dar mismo intervalo");
        assert!(a.0 < 19.5 && a.1 > 19.5, "el IC debe cubrir la media");
    }

    #[test]
    fn rng_below_en_rango() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            assert!(r.below(13) < 13);
        }
    }
}

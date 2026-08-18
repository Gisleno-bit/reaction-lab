//! Nucleo de la herramienta.

pub mod csv;
pub mod datos;
pub mod estadistica;
pub mod informe;
pub mod nucleo;
pub mod prereg;
pub mod sha256;

pub use nucleo::{analizar, Altura, Muestra, Resultado, Situacion, Umbrales};

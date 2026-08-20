//! Reaction Lab — analisis de tiempos de reaccion por comparacion contra baseline.
//!
//! Mide una DESVIACION ESTADISTICA. No inspecciona ninguna maquina, no detecta
//! software y no mira dentro del proceso de nadie: por red solo llegan los
//! inputs del rival, nunca su proceso.
//!
//! # El problema del estimulo, resuelto
//!
//! No hace falta saber en que frame un low se vuelve visualmente reconocible.
//! Ese retardo es una constante desconocida pero FIJA para cada move. Midiendo
//! siempre desde el frame 1 de la animacion y comparando al sujeto contra el
//! baseline EN EL MISMO MOVE, la constante es identica en ambos lados y se
//! cancela sola.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod reaccion;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

//! Lector CSV minimo pero correcto: maneja BOM, CRLF y campos entrecomillados
//! con comillas dobladas (lo que exporta Excel).

use std::collections::HashMap;

#[derive(Debug)]
pub struct Tabla {
    pub cabeceras: Vec<String>,
    pub filas: Vec<Vec<String>>,
}

impl Tabla {
    /// Indice de columna por nombre, insensible a mayusculas y espacios.
    pub fn indice(&self, nombre: &str) -> Option<usize> {
        let n = nombre.trim().to_lowercase();
        self.cabeceras
            .iter()
            .position(|c| c.trim().to_lowercase() == n)
    }

    pub fn como_mapas(&self) -> Vec<HashMap<String, String>> {
        self.filas
            .iter()
            .map(|fila| {
                self.cabeceras
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        (
                            c.trim().to_lowercase(),
                            fila.get(i).cloned().unwrap_or_default(),
                        )
                    })
                    .collect()
            })
            .collect()
    }
}

/// Parte un texto CSV. Ignora lineas completamente vacias.
pub fn parsear(texto: &str) -> Tabla {
    let texto = texto.strip_prefix('\u{feff}').unwrap_or(texto);
    let mut filas: Vec<Vec<String>> = Vec::new();
    let mut fila: Vec<String> = Vec::new();
    let mut campo = String::new();
    let mut en_comillas = false;
    let mut chars = texto.chars().peekable();

    while let Some(c) = chars.next() {
        if en_comillas {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    campo.push('"');
                } else {
                    en_comillas = false;
                }
            } else {
                campo.push(c);
            }
            continue;
        }
        match c {
            '"' => en_comillas = true,
            ',' => fila.push(std::mem::take(&mut campo)),
            '\r' => {}
            '\n' => {
                fila.push(std::mem::take(&mut campo));
                if !(fila.len() == 1 && fila[0].trim().is_empty()) {
                    filas.push(std::mem::take(&mut fila));
                } else {
                    fila.clear();
                }
            }
            _ => campo.push(c),
        }
    }
    if !campo.is_empty() || !fila.is_empty() {
        fila.push(campo);
        if !(fila.len() == 1 && fila[0].trim().is_empty()) {
            filas.push(fila);
        }
    }

    if filas.is_empty() {
        return Tabla {
            cabeceras: Vec::new(),
            filas: Vec::new(),
        };
    }
    let cabeceras = filas.remove(0);
    Tabla { cabeceras, filas }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basico() {
        let t = parsear("a,b\n1,2\n3,4\n");
        assert_eq!(t.cabeceras, vec!["a", "b"]);
        assert_eq!(t.filas.len(), 2);
        assert_eq!(t.filas[1], vec!["3", "4"]);
    }

    #[test]
    fn bom_y_crlf() {
        let t = parsear("\u{feff}a,b\r\n1,2\r\n");
        assert_eq!(t.cabeceras, vec!["a", "b"]);
        assert_eq!(t.filas[0], vec!["1", "2"]);
    }

    #[test]
    fn comillas_con_coma_y_dobles() {
        let t = parsear("a,b\n\"uno, dos\",\"di \"\"hola\"\"\"\n");
        assert_eq!(t.filas[0][0], "uno, dos");
        assert_eq!(t.filas[0][1], "di \"hola\"");
    }

    #[test]
    fn campos_vacios_se_conservan() {
        let t = parsear("a,b,c\n1,,3\n");
        assert_eq!(t.filas[0], vec!["1", "", "3"]);
    }

    #[test]
    fn ultima_linea_sin_salto() {
        let t = parsear("a,b\n1,2");
        assert_eq!(t.filas.len(), 1);
    }

    #[test]
    fn lineas_vacias_ignoradas() {
        let t = parsear("a,b\n1,2\n\n3,4\n");
        assert_eq!(t.filas.len(), 2);
    }

    #[test]
    fn indice_insensible_a_mayusculas() {
        let t = parsear("Player, Tier\nx,pro\n");
        assert_eq!(t.indice("player"), Some(0));
        assert_eq!(t.indice("TIER"), Some(1));
        assert_eq!(t.indice("nope"), None);
    }
}

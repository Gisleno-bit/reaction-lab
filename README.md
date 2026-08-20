# Reaction Lab

Análisis de reacciones a lows en juegos de lucha. **Sin necesidad de jugador de control.**

Rust, cero dependencias. Binario de ~500 KB, compila en segundos, todo el cálculo —incluido el SHA-256— auditable dentro del repo.

---

## En qué se apoya

Las versiones anteriores comparaban al sujeto contra un baseline de pros. Eso siempre era discutible: *"es que él es mejor"*. Este modelo se apoya en dos cosas que nadie puede rebatir:

**Física.** Por debajo de 21 frames no es reacción. Da igual quién seas y cuánto entrenes.

**Aritmética.** En un true 50/50 hay que adivinar, y adivinando el techo es 50%. La probabilidad de superarlo por suerte se calcula exacta con un binomial.

Y mide dos firmas que un humano no puede producir:

**Margen constante hasta el impacto.** Un humano llega con margen variable. Un programa que dispara en `impacto − 1` llega siempre con el mismo. Y un humano no tiene forma de saber cuál es el último frame: para clavarlo hay que conocer el startup y contar en tiempo real.

**Activación selectiva.** Los cheats son configurables por condición, así que el contexto se parte por todas las que un cheat puede leer: ronda, marcador, vida y reloj.

---

## Lo que más importa: no acusar a un limpio

Un falso positivo es el peor fallo posible de esta herramienta. Durante el desarrollo salieron tres, y los tres están cerrados con tests de regresión:

| Fallo | Causa | Arreglo |
|---|---|---|
| Marcaba al humano por reaccionar "demasiado rápido" | Medía la física en los 50/50, donde el jugador **adivina** y se agacha por adelantado | La física se mide **solo en situaciones de timing** |
| Marcaba saltos de contexto que eran azar | Comparaba porcentajes brutos | Test z de dos proporciones |
| Seguía marcando al humano con p < 0,01 | Probaba 4 dimensiones × varios tramos y reportaba **la diferencia mayor**: p-hacking dentro de la herramienta | Corrección de Bonferroni sobre todas las comparaciones |

El test `un_humano_limpio_no_dispara_ninguna_bandera` corre siete semillas distintas y exige cero banderas en todas.

---

## Interfaz grafica

```bash
reaction-lab interfaz
```

Abre `http://127.0.0.1:8787` en el navegador. Tres pasos en pantalla: sellar los umbrales, meter las muestras (a mano, importando un CSV o cargando un ejemplo) y analizar. El informe sale renderizado y se descarga con un boton.

**Toda la estadistica la hace el nucleo de Rust**, el mismo que usa la CLI y el que cubren los tests. La pagina solo recoge datos y pinta el resultado. Si la interfaz recalculara medias por su cuenta existirian dos implementaciones de lo mismo, y en cuanto se separasen un decimal no podrias defender que cifra es la buena.

Escucha **solo en 127.0.0.1**: no se expone a la red ni a internet. Los datos de un caso asi no deben salir de la maquina de quien lo analiza.

Si cargas un pre-registro sellado, **sus umbrales mandan** sobre los de la pantalla y los campos se bloquean. Ese es justo el punto de tener un sello.

---

## Uso por linea de comandos

```bash
reaction-lab sellar --sujeto <nombre> --autor "<organización>" -o pr.txt
reaction-lab plantilla -o muestras.csv
reaction-lab analizar --csv muestras.csv --sujeto <nombre> \
    --preregistro pr.txt --solo-offline -o informe.html
reaction-lab verificar pr.txt
```

Para ver cómo se lee cada firma sin capturar nada:

```bash
reaction-lab demo --perfil humano      # sale limpio
reaction-lab demo --perfil script      # auto-block: las cuatro firmas
reaction-lab demo --perfil selectivo   # cheat configurado para ronda 3 y poca vida
```

### Formato del CSV

| Columna | Significado |
|---|---|
| `situacion` | `true5050` (hay que adivinar) o `timing` (se puede reaccionar) |
| `move_id` | Identificador del move |
| `altura` | `low` o `mid` |
| `startup` | Frame en que el move golpea. Sin esto no hay margen |
| `latency` | Frames desde el **frame 1 de la animación** al input de guardia baja |
| `agachado` | 1/0. En un low = bloqueo; en un mid = se lo comió |
| `vida_pct` | Vida del defensor, 0–100 |
| `ronda` | 1, 2, 3… |
| `rondas_propias` / `rondas_rival` | Marcador **antes** de esta ronda |
| `seg_restantes` | Reloj tal y como se ve en pantalla |
| `online` | 1/0. Nunca mezcles online y offline |

Una fila **por cada golpe lanzado**, lows y mids. Los mids son imprescindibles: sin ellos no se ve lo que pierde al equivocarse.

Rellena el contexto en **todas** las filas, no solo en las sospechosas. Si solo anotas los momentos raros, encontrarás el patrón que buscabas aunque no exista.

---

## La clasificación es tuya, y ahí está toda la responsabilidad

Marca **true 50/50** solo cuando de verdad no haya opción de reaccionar: mismo stance, mismo arranque de animación, ambas opciones lo bastante rápidas. En cuanto haya un tell distinguible o el low sea de 21 frames o más, es **timing**, y ahí un jugador bueno puede acertar mucho sin trampa.

Si te la cuelan, te la cuelan por ahí. Conviene que esa clasificación la firme alguien de la escena que no esté metido en la acusación, **antes** de mirar los resultados.

---

## Pre-registro

Si eliges los umbrales *después* de ver los datos, cualquiera desmonta el análisis en dos frases. `reaction-lab sellar` genera un documento de texto plano canónico con un SHA-256; publícalo antes de correr el análisis. Si alguien afloja un umbral después de sellar, `verificar` y `analizar` fallan con error.

---

## Limitaciones

Mide **desviaciones estadísticas y límites físicos**. No inspecciona ninguna máquina ni detecta software — eso es imposible desde fuera: por red solo llegan los inputs del rival, nunca su proceso.

- Un resultado sin banderas **no prueba inocencia**.
- Un resultado con banderas **no prueba culpabilidad**: prueba que algo se sale de lo humano o de lo aritméticamente posible.
- No publiques un nombre hasta que el análisis esté cerrado y la persona haya podido responder.

---

## Desarrollo

```bash
cargo test          # 55 tests, sin red ni dependencias
cargo build --release
cargo run -- demo --perfil script
```

Los binarios de Windows, Linux y macOS se compilan en GitHub Actions al hacer push de un tag `v*`.

## Licencia

MIT — ver [LICENSE](LICENSE).

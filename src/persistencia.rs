use crate::observer::Observer;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Result, Write};
use std::iter::FromIterator;

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use crate::base_de_datos::TipoRedis;
use crate::valor::Valor;

const STRING: &str = "STRING";
const LIST: &str = "LIST";
const SET: &str = "SET";
const EX: &str = "EX";
const SEPARADOR: &str = ":";

/// Representa un mensaje que puede enviar el Persistidor al PersistidorHandler
pub enum MensajePersistencia {
    /// Encapsula la tabla a persistir
    Info(HashMap<String, Valor>),
    /// Encapsula el Archivo donde se debe persistir la base de datos
    ArchivoAPersistir(String),
    /// Cierra el hilo donde se esta ejecutando el PersistidorHandler
    Cerrar,
}

/// Entidad que se encarga de correr en un hilo y persistir la base de datos a traves de mensajes con el Persistidor
pub struct PersistidorHandler {
    archivo: String,
    intervalo: Duration,
    instante: Instant,
    receptor: Receiver<MensajePersistencia>,
}

impl PersistidorHandler {
    /// Instancia un manejador listo para recibir mensajes
    ///
    /// # Argumentos
    ///
    /// * `archivo` - string donde se va a persistir la base de datos
    /// * `intervalo` - intervalo de tiempo a traves del cual se va a persistir
    /// * `receptor` - Receiver de mensajes asociado al channel del Persistidor
    pub fn new(archivo: String, intervalo: u64, receptor: Receiver<MensajePersistencia>) -> Self {
        PersistidorHandler {
            archivo,
            receptor,
            instante: Instant::now(),
            intervalo: Duration::from_secs(intervalo),
        }
    }

    /// Ejecuta al manejador esperando mensajes
    ///
    /// ```no_run
    /// let (tx_pers, rx_pers) = channel();
    /// let mut pers_handler = PersistidorHandler::new(config.dbfilename(), 1, rx_pers);
    ///
    /// let hilo_pers = thread::spawn(move || {
    ///     pers_handler.persistir();
    /// });
    /// ```
    pub fn persistir(&mut self) {
        while let Ok(mensaje) = self.receptor.recv() {
            match mensaje {
                MensajePersistencia::Info(a_persistir) => {
                    if self.instante.elapsed() >= self.intervalo {
                        //persisto
                        let mut vector: Vec<String> = vec![];
                        for (key, val) in a_persistir.iter() {
                            vector.push(guardar_clave_valor(
                                key.to_string(),
                                val.get(),
                                val.get_tiempo(),
                            ));
                        }
                        match guardar_en_archivo(&self.archivo, vector) {
                            Ok(_) => (),
                            Err(_) => break,
                        };
                        self.instante = Instant::now();
                    }
                }

                MensajePersistencia::ArchivoAPersistir(a) => self.archivo = a,

                MensajePersistencia::Cerrar => break,
            };
        }
    }
}

/// Representa al mensajero que se comunica con el manejador para persistir la base de datos
#[derive(Debug, Clone)]
pub struct Persistidor {
    persistidor: Sender<MensajePersistencia>,
}

impl Persistidor {
    /// Instancia un persistidor para enviar mensajes
    ///
    /// # Argumentos
    ///
    /// * `persistidor` - Sender de MensajePersistencia asociado la channel de PersistidorHandler
    pub fn new(persistidor: Sender<MensajePersistencia>) -> Self {
        Persistidor { persistidor }
    }

    pub fn persistir(&self, base_de_datos: HashMap<String, Valor>) {
        if self
            .persistidor
            .send(MensajePersistencia::Info(base_de_datos))
            .is_ok()
        {}
    }

    /// Cambia el archivo donde se persiste la base de datos
    pub fn cambiar_archivo(&self, ruta_nueva: String) {
        if self
            .persistidor
            .send(MensajePersistencia::ArchivoAPersistir(ruta_nueva))
            .is_ok()
        {}
    }
}

/// El persistidor es un observador que espera a que la base de datos notifique cuando se produjo un cambio importante
impl Observer for Persistidor {
    /// Al actualizarse envia la nueva base de datos a persistir
    fn actualizar(&self, bdd: HashMap<String, Valor>) {
        self.persistir(bdd);
    }
}

/// Crea una cadena con una codificacion especifica para persistir a partir de una clave y un valor
fn guardar_clave_valor(clave: String, valor: Option<&TipoRedis>, time: Option<Duration>) -> String {
    match (valor, time) {
        (Some(TipoRedis::Str(valor)), Some(duration)) => {
            STRING.to_string()
                + SEPARADOR
                + &clave
                + SEPARADOR
                + valor
                + SEPARADOR
                + EX
                + SEPARADOR
                + &(duration.as_secs().to_string())
        }

        (Some(TipoRedis::Str(valor)), None) => {
            STRING.to_string() + SEPARADOR + &clave + SEPARADOR + valor
        }

        (Some(TipoRedis::Lista(lista)), Some(duration)) => {
            let mut persistencia_lista = LIST.to_string() + SEPARADOR + &clave;
            for valor in lista.iter() {
                persistencia_lista += &(SEPARADOR.to_string() + valor);
            }
            persistencia_lista +=
                &(SEPARADOR.to_string() + EX + SEPARADOR + &(duration.as_secs().to_string()));
            persistencia_lista
        }

        (Some(TipoRedis::Lista(lista)), None) => {
            let mut persistencia_lista = LIST.to_string() + SEPARADOR + &clave;
            for valor in lista.iter() {
                persistencia_lista += &(SEPARADOR.to_string() + valor);
            }
            persistencia_lista
        }

        (Some(TipoRedis::Set(set)), Some(duration)) => {
            let mut persistencia_set = SET.to_string() + SEPARADOR + &clave;
            for valor in set.iter() {
                persistencia_set += &(SEPARADOR.to_string() + valor);
            }
            persistencia_set +=
                &(SEPARADOR.to_string() + EX + SEPARADOR + &(duration.as_secs().to_string()));
            persistencia_set
        }
        (Some(TipoRedis::Set(set)), None) => {
            let mut persistencia_set = SET.to_string() + SEPARADOR + &clave;
            for valor in set.iter() {
                persistencia_set += &(SEPARADOR.to_string() + valor);
            }
            persistencia_set
        }
        _ => String::new(),
    }
}

fn guardar_en_archivo(archivo: &str, instrucciones: Vec<String>) -> Result<()> {
    let mut archivo = match OpenOptions::new().write(true).create(true).open(archivo) {
        Ok(a) => a,
        Err(e) => return Err(e),
    };

    for instruccion in instrucciones.iter() {
        if let Err(e) = writeln!(archivo, "{}", instruccion) {
            println!("{:?}", e);
        }
    }
    Ok(())
}

/// Lee el archivo de persistencia y crea una nuevo hashmap a partir de el
pub fn levantar_tabla(archivo_persistencia: String) -> HashMap<String, Valor> {
    let mut hashmap = HashMap::<String, Valor>::new();

    let archivo = match File::open(archivo_persistencia) {
        Ok(archivo) => archivo,
        Err(_) => return hashmap,
    };

    let reader = BufReader::new(archivo);
    let mut lineas = reader.lines();
    while let Some(Ok(line)) = lineas.next() {
        if line.is_empty() {
            continue;
        }
        let mut elemento: Vec<&str> = line.split(':').collect();

        if elemento.contains(&"STRING") {
            let mut valor = Valor::no_expirable(TipoRedis::Str(elemento[2].to_string()));

            if es_expirable(elemento.clone()) {
                let tiempo = obtener_tiempo_expiracion(elemento.clone(), "EX").unwrap_or(0);
                valor = Valor::expirable(TipoRedis::Str(elemento[2].to_string()), tiempo);
            }
            hashmap.insert(elemento[1].to_string(), valor);
        } else if elemento.contains(&"LIST") {
            elemento.remove(0);
            let clave = elemento.remove(0).to_string();
            let mut valor = Valor::no_expirable(TipoRedis::Lista(
                elemento.iter().map(|x| x.to_string()).collect(),
            ));

            if es_expirable(elemento.clone()) {
                let tiempo = obtener_tiempo_expiracion(elemento.clone(), "EX").unwrap_or(0);

                valor = Valor::expirable(
                    TipoRedis::Lista(elemento.iter().map(|x| x.to_string()).collect()),
                    tiempo,
                );
            }
            hashmap.insert(clave, valor);
        } else if elemento.contains(&"SET") {
            elemento.remove(0);
            let clave = elemento.remove(0).to_string();
            let mut valor = Valor::no_expirable(TipoRedis::Set(HashSet::from_iter(
                elemento
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>(),
            )));
            if es_expirable(elemento.clone()) {
                let tiempo = obtener_tiempo_expiracion(elemento.clone(), "EX").unwrap_or(0);

                valor = Valor::expirable(
                    TipoRedis::Set(HashSet::from_iter(
                        elemento
                            .iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<String>>(),
                    )),
                    tiempo,
                );
            }
            hashmap.insert(clave, valor);
        } else {
            continue;
        }
    }
    hashmap
}

fn es_expirable(parametros: Vec<&str>) -> bool {
    parametros.contains(&"EX")
}

fn obtener_tiempo_expiracion(parametros: Vec<&str>, support: &str) -> Option<u64> {
    match parametros.rsplit(|p| p == &support.to_string()).next() {
        Some(c) => match c[0].parse::<u64>() {
            Ok(num) => Some(num),
            Err(_) => None,
        },
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn inserto_varios_strings_en_hash_map_y_guardar_clave_valor_devuelve_el_mensaje_para_volver_a_cargarlos(
    ) {
        let mut map = HashMap::new();
        map.insert(
            "UnaClave1",
            Valor::no_expirable(TipoRedis::Str("UnValor".to_string())),
        );
        map.insert(
            "UnaClave2",
            Valor::no_expirable(TipoRedis::Str("UnValor".to_string())),
        );
        map.insert(
            "UnaClave3",
            Valor::no_expirable(TipoRedis::Str("UnValor".to_string())),
        );

        let mut vector: Vec<String> = vec![];
        for (key, val) in map.iter() {
            vector.push(guardar_clave_valor(
                key.to_string(),
                val.get(),
                val.get_tiempo(),
            ));
        }
        assert!(vector.contains(&"STRING:UnaClave1:UnValor".to_string()));
        assert!(vector.contains(&"STRING:UnaClave2:UnValor".to_string()));
        assert!(vector.contains(&"STRING:UnaClave3:UnValor".to_string()));
    }

    #[test]
    fn inserto_varios_tipo_redis_en_hash_map_y_guardar_clave_valor_devuelve_el_mensaje_para_volver_a_cargarlos(
    ) {
        let mut map = HashMap::new();
        map.insert(
            "UnaClave1",
            Valor::no_expirable(TipoRedis::Str("UnValor".to_string())),
        );
        map.insert(
            "UnaClave2",
            Valor::no_expirable(TipoRedis::Str("UnValor".to_string())),
        );

        let mut lista = TipoRedis::Lista(Vec::new());

        match lista {
            TipoRedis::Lista(ref mut lista) => {
                lista.push("PRIMER_VALOR".to_string());
                lista.push("SEGUNDO_VALOR".to_string());
                lista.push("TERCER_VALOR".to_string());
            }
            _ => {}
        }

        map.insert("milista", Valor::no_expirable(lista));

        let mut vector: Vec<String> = vec![];
        for (key, val) in map.iter() {
            vector.push(guardar_clave_valor(
                key.to_string(),
                val.get(),
                val.get_tiempo(),
            ));
        }
        assert!(vector.contains(&"STRING:UnaClave1:UnValor".to_string()));
        assert!(vector.contains(&"STRING:UnaClave2:UnValor".to_string()));
        assert!(
            vector.contains(&"LIST:milista:PRIMER_VALOR:SEGUNDO_VALOR:TERCER_VALOR".to_string())
        );
    }

    #[test]
    fn inserto_varios_strings_con_persistencia_en_hash_map_y_guardar_clave_valor_devuelve_el_mensaje_para_volver_a_cargarlos(
    ) {
        let mut map = HashMap::new();
        map.insert(
            "UnaClave1",
            Valor::expirable(TipoRedis::Str("UnValor".to_string()), 3000),
        );
        map.insert(
            "UnaClave2",
            Valor::expirable(TipoRedis::Str("UnValor".to_string()), 3000),
        );
        map.insert(
            "UnaClave3",
            Valor::expirable(TipoRedis::Str("UnValor".to_string()), 3000),
        );

        let mut lista = TipoRedis::Lista(Vec::new());

        match lista {
            TipoRedis::Lista(ref mut lista) => {
                lista.push("PRIMER_VALOR".to_string());
                lista.push("SEGUNDO_VALOR".to_string());
                lista.push("TERCER_VALOR".to_string());
            }
            _ => {}
        }

        map.insert("milista", Valor::expirable(lista, 4500));

        let mut vector: Vec<String> = vec![];
        for (key, val) in map.iter() {
            vector.push(guardar_clave_valor(
                key.to_string(),
                val.get(),
                val.get_tiempo(),
            ));
        }
        assert!(vector.contains(&"STRING:UnaClave1:UnValor:EX:3000".to_string()));
        assert!(vector.contains(&"STRING:UnaClave2:UnValor:EX:3000".to_string()));
        assert!(vector.contains(&"STRING:UnaClave3:UnValor:EX:3000".to_string()));
        assert!(vector
            .contains(&"LIST:milista:PRIMER_VALOR:SEGUNDO_VALOR:TERCER_VALOR:EX:4500".to_string()));
    }
}

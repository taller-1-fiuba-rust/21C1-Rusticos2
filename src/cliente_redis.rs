use crate::base_de_datos::ResultadoRedis;
use crate::cliente::{TipoCliente, Token};
use crate::comando_info::ComandoInfo;
use crate::parser::{parsear_respuesta, Parser};
use crate::redis_error::RedisError;
use std::time::{Duration, Instant};

use std::fmt;
use std::io::Write;
use std::net::TcpStream;

/// Representa a un Cliente que envia mensajes utilizando el protocolo redis
pub struct ClienteRedis {
    id: Token,
    canales: usize,
    timeout: Option<Duration>,
    ultimo_mensaje: Instant,
    socket: Option<TcpStream>,
}

impl ClienteRedis {
    /// Se instancia un ClienteRedis en condiciones de procesar mensajes
    ///
    /// # Argumentos
    ///
    /// * `token` - id unica
    /// * `timeout` - intervalo de tiempo a esperar a que el usuario envie un mensaje
    /// * `socket` - stream especifico del cliente
    pub fn new(id: Token, timeout: u64, stream: TcpStream) -> Self {
        let duracion = match timeout {
            0 => None,
            t => Some(Duration::from_secs(t)),
        };

        ClienteRedis {
            id,
            canales: 0,
            timeout: duracion,
            ultimo_mensaje: Instant::now(),
            socket: Some(stream),
        }
    }

    fn obtener_socket(&self) -> Option<TcpStream> {
        let socket = match &self.socket {
            None => return None,
            Some(t) => t,
        };

        match socket.try_clone() {
            Ok(t) => Some(t),
            Err(_) => None,
        }
    }
}

impl TipoCliente for ClienteRedis {
    /// Encapsula el obtener el comando en particular
    ///
    /// # Resultados
    ///
    /// * `Ok(Some(c))` - Se obtiene el comando enviado correctamente
    /// * `Err(e)` - Se produjo un error al la hora de obtener el comando
    fn obtener_comando(&mut self) -> Result<Option<ComandoInfo>, RedisError> {
        let stream = match self.obtener_socket() {
            Some(s) => s,
            None => return Err(RedisError::Coneccion),
        };
        let parser = Parser::new(stream);

        match parser.parsear_stream() {
            Ok(orden) => Ok(Some(orden)),
            Err(_) => Err(RedisError::Server),
        }
    }

    fn obtener_addr(&self) -> String {
        let socket = match &self.socket {
            None => return format!("Token: {}", self.id),
            Some(t) => t,
        };

        match socket.local_addr() {
            Ok(a) => format!("Token: {} IP: ", self.id) + &a.to_string(),
            Err(_) => format!("Token: {}", self.id),
        }
    }

    fn envio_informacion(&self) -> bool {
        let socket = match &self.socket {
            None => return false,
            Some(t) => t,
        };

        match socket.peek(&mut [0; 128]) {
            Ok(len) => len > 0,
            Err(_) => false,
        }
    }

    fn esta_conectado(&self) -> bool {
        let socket = match &self.socket {
            None => return false,
            Some(t) => t,
        };

        let esta_conectado = match socket.peek(&mut [0; 128]) {
            Ok(len) => len != 0,
            Err(_) => false,
        };

        let paso_el_timeout = match self.timeout {
            Some(d) => self.ultimo_mensaje.elapsed() > d,
            None => false,
        };

        esta_conectado && !paso_el_timeout
    }

    fn enviar_resultado(&mut self, resultado: &ResultadoRedis) -> Result<(), RedisError> {
        let mensaje = parsear_respuesta(resultado);
        self.enviar_mensaje(mensaje)
    }

    fn enviar_mensaje(&mut self, mensaje: String) -> Result<(), RedisError> {
        self.ultimo_mensaje = Instant::now();

        let socket = match &mut self.socket {
            None => return Err(RedisError::Coneccion),
            Some(t) => t,
        };

        match socket.write(mensaje.as_bytes()) {
            Ok(_) => Ok(()),
            Err(_) => Err(RedisError::Coneccion),
        }
    }
    fn obtener_token(&self) -> Token {
        self.id
    }

    fn soporta_comando(&self, _comando: &str) -> bool {
        true
    }
}

impl Clone for ClienteRedis {
    fn clone(&self) -> Self {
        ClienteRedis {
            id: self.id,
            canales: self.canales,
            timeout: self.timeout,
            ultimo_mensaje: self.ultimo_mensaje,
            socket: self.obtener_socket(),
        }
    }
}

impl PartialEq for ClienteRedis {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ClienteRedis {}

impl fmt::Debug for ClienteRedis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClienteRedis")
            .field("id", &self.id)
            .field("timeout", &self.timeout)
            .field("ultimo_mensaje", &self.ultimo_mensaje)
            .field("socket", &self.socket)
            .finish()
    }
}

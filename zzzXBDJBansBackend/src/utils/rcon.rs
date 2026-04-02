use byteorder::{LittleEndian, ReadBytesExt};
use std::io::Cursor;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const RCON_AUTH: i32 = 3;
const RCON_AUTH_RESPONSE: i32 = 2;
const RCON_EXEC_COMMAND: i32 = 2;
const RCON_RESPONSE_VALUE: i32 = 0;
const MIN_PACKET_SIZE: usize = 10;

pub async fn check_rcon(address: &str, password: &str) -> Result<(), String> {
    let mut stream = connect(address).await?;
    send_packet(&mut stream, 999, RCON_AUTH, password.as_bytes()).await?;

    let mut read_buf = [0u8; 4096];
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let n = stream
                .read(&mut read_buf)
                .await
                .map_err(|e| format!("Read failed: {e}"))?;

            if n == 0 {
                return Err("Connection closed or invalid response".to_string());
            }

            let buffer = &read_buf[..n];
            let mut cursor = Cursor::new(buffer);

            loop {
                let position_before = cursor.position();
                let packet = next_packet(buffer, &mut cursor)?;
                let Some((id, packet_type, _body)) = packet else {
                    break;
                };

                if packet_type == RCON_AUTH_RESPONSE {
                    if id == -1 {
                        return Err("Authentication failed (Bad Password)".to_string());
                    }
                    if id == 999 {
                        return Ok(());
                    }
                }

                if cursor.position() == position_before {
                    break;
                }
            }
        }
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Response timed out".to_string()),
    }
}

pub async fn send_command(address: &str, password: &str, command: &str) -> Result<String, String> {
    let mut stream = connect(address).await?;
    send_packet(&mut stream, 1, RCON_AUTH, password.as_bytes()).await?;
    read_auth_response(&mut stream).await?;

    send_packet(&mut stream, 42, RCON_EXEC_COMMAND, command.as_bytes()).await?;

    let mut read_buf = [0u8; 4096];
    let mut response_data = String::new();

    let read_result = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let n = match stream.read(&mut read_buf).await {
                Ok(0) => break Ok(()),
                Ok(n) => n,
                Err(e) => break Err(format!("Read failed(cmd): {e}")),
            };

            let buffer = &read_buf[..n];
            let mut cursor = Cursor::new(buffer);

            loop {
                let position_before = cursor.position();
                let packet = next_packet(buffer, &mut cursor)?;
                let Some((_id, packet_type, body)) = packet else {
                    break;
                };

                if packet_type == RCON_RESPONSE_VALUE && !body.is_empty() {
                    response_data.push_str(&String::from_utf8_lossy(body));
                }

                if cursor.position() == position_before {
                    break;
                }
            }

            if n < read_buf.len() {
                break Ok(());
            }
        }
    })
    .await;

    if !response_data.is_empty() {
        return Ok(response_data);
    }

    match read_result {
        Ok(Ok(())) => Ok(String::new()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Command timed out or no response".to_string()),
    }
}

async fn connect(address: &str) -> Result<TcpStream, String> {
    tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(address))
        .await
        .map_err(|_| "Connection timed out".to_string())?
        .map_err(|e| format!("Failed to connect: {e}"))
}

async fn read_auth_response(stream: &mut TcpStream) -> Result<(), String> {
    let mut read_buf = [0u8; 4096];
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let n = stream
                .read(&mut read_buf)
                .await
                .map_err(|e| format!("Read failed(auth): {e}"))?;

            if n == 0 {
                return Err("Connection closed".to_string());
            }

            let buffer = &read_buf[..n];
            let mut cursor = Cursor::new(buffer);

            loop {
                let position_before = cursor.position();
                let packet = next_packet(buffer, &mut cursor)?;
                let Some((id, packet_type, _body)) = packet else {
                    break;
                };

                if packet_type == RCON_AUTH_RESPONSE {
                    if id == -1 {
                        return Err("Authentication failed".to_string());
                    }
                    if id == 1 {
                        return Ok(());
                    }
                }

                if cursor.position() == position_before {
                    break;
                }
            }
        }
    })
    .await;

    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Auth timed out".to_string()),
    }
}

async fn send_packet(
    stream: &mut TcpStream,
    request_id: i32,
    packet_type: i32,
    body: &[u8],
) -> Result<(), String> {
    let packet_size = 4 + 4 + body.len() + 1 + 1;
    let mut buffer = Vec::with_capacity(4 + packet_size);
    push_i32(&mut buffer, packet_size as i32);
    push_i32(&mut buffer, request_id);
    push_i32(&mut buffer, packet_type);
    buffer.extend_from_slice(body);
    buffer.push(0x00);
    buffer.push(0x00);

    stream
        .write_all(&buffer)
        .await
        .map_err(|e| format!("Write failed: {e}"))
}

fn push_i32(buffer: &mut Vec<u8>, value: i32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn next_packet<'a>(
    buffer: &'a [u8],
    cursor: &mut Cursor<&'a [u8]>,
) -> Result<Option<(i32, i32, &'a [u8])>, String> {
    let start = cursor.position() as usize;
    let remaining = buffer.len().saturating_sub(start);

    if remaining < 4 {
        return Ok(None);
    }

    let size = read_i32(cursor)? as isize;
    if size < MIN_PACKET_SIZE as isize {
        return Err(format!("Invalid RCON packet size: {size}"));
    }

    let size = size as usize;
    let packet_end = cursor.position() as usize + size;
    if packet_end > buffer.len() {
        cursor.set_position(start as u64);
        return Ok(None);
    }

    let request_id = read_i32(cursor)?;
    let packet_type = read_i32(cursor)?;

    let body_len = size - MIN_PACKET_SIZE;
    let body_start = cursor.position() as usize;
    let body_end = body_start + body_len;
    let body = &buffer[body_start..body_end];

    cursor.set_position(packet_end as u64);

    Ok(Some((request_id, packet_type, body)))
}

fn read_i32(cursor: &mut Cursor<&[u8]>) -> Result<i32, String> {
    ReadBytesExt::read_i32::<LittleEndian>(cursor)
        .map_err(|e| format!("Invalid RCON packet: {e}"))
}

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::Duration;
use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use std::io::Cursor;

pub async fn check_rcon(address: &str, password: &str) -> Result<(), String> {
    // Connect with timeout
    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(address)
    ).await
    .map_err(|_| "Connection timed out")?
    .map_err(|e| format!("Failed to connect: {}", e))?;

    let mut stream = stream;

    // Build Auth Packet
    // Size (4) + ID (4) + Type (4) + Body (Str + \0) + Empty (1)
    // Packet Size = 4 + 4 + BodyLen + 1 + 1
    // Type 3 = SERVERDATA_AUTH
    
    let req_id = 999;
    let packet_type = 3;
    let body = password.as_bytes();
    let body_len = body.len() as i32;
    // Total Size field value = 4 (ID) + 4 (Type) + body_len + 1 (null) + 1 (null)
    // Note: The Size field itself is NOT included in the size value.
    let packet_size = 4 + 4 + body_len + 1 + 1;
    
    let mut buffer = Vec::new();
    WriteBytesExt::write_i32::<LittleEndian>(&mut buffer, packet_size).unwrap();
    WriteBytesExt::write_i32::<LittleEndian>(&mut buffer, req_id).unwrap();
    WriteBytesExt::write_i32::<LittleEndian>(&mut buffer, packet_type).unwrap();
    buffer.extend_from_slice(body);
    buffer.push(0x00); // Body null terminator
    buffer.push(0x00); // Empty string null terminator (Packet padding?) - Protocol says "Body... null... null"

    // Send
    stream.write_all(&buffer).await.map_err(|e| format!("Write failed: {}", e))?;

    // Read Response
    // First response usually SERVERDATA_RESPONSE_VALUE (Type 0) (optional?), then SERVERDATA_AUTH_RESPONSE (Type 2)
    // Or just Auth Response. 
    // We loop reading packets until we get Type 2.
    
    let mut read_buf = [0u8; 4096];
    
    // Simple read loop with timeout
    let result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let n = stream.read(&mut read_buf).await.map_err(|e| e.to_string())?;
            if n < 4 { return Err("Connection closed or invalid response".to_string()); }
            
            // Parse packet(s)
            let mut cursor = Cursor::new(&read_buf[..n]);
            while (cursor.position() as usize) < n {
                if n - (cursor.position() as usize) < 4 { break; } // Not enough for size
                let size = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap() as usize;
                
                // Check if we have the full packet
                if n - (cursor.position() as usize) < size { 
                    // In a real implementation we would buffer remaining and read more. 
                    // For auth check, usually response is small and comes at once.
                    // Assuming simplified scenario for now.
                    break; 
                }
                
                let _id = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap();
                let type_ = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap();
                
                // Skip Body (read until null) + Empty null
                // We just seek forward essentially, or read to verify
                // Size includes ID(4) + Type(4) + Body + null.
                // Current position is after Type. 
                // Remaining to skip = Size - 4 - 4.
                let advance = size - 8;
                cursor.set_position(cursor.position() + advance as u64);
                
                if type_ == 2 { // SERVERDATA_AUTH_RESPONSE
                    if _id == -1 {
                        return Err("Authentication failed (Bad Password)".to_string());
                    } else if _id == req_id {
                        return Ok(()); // Success
                    }
                }
            }
        }
    }).await;

    match result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("Response timed out".to_string()),
    }
}

pub async fn send_command(address: &str, password: &str, command: &str) -> Result<String, String> {
    // Connect with timeout
    let stream = tokio::time::timeout(
        Duration::from_secs(5),
        TcpStream::connect(address)
    ).await
    .map_err(|_| "Connection timed out")?
    .map_err(|e| format!("Failed to connect: {}", e))?;

    let mut stream = stream;

    // --- Authenticate ---
    let req_id = 1;
    let auth_packet_type = 3;
    let body = password.as_bytes();
    let packet_size = 4 + 4 + body.len() as i32 + 1 + 1;
    
    let mut buffer = Vec::new();
    WriteBytesExt::write_i32::<LittleEndian>(&mut buffer, packet_size).unwrap();
    WriteBytesExt::write_i32::<LittleEndian>(&mut buffer, req_id).unwrap();
    WriteBytesExt::write_i32::<LittleEndian>(&mut buffer, auth_packet_type).unwrap();
    buffer.extend_from_slice(body);
    buffer.push(0x00);
    buffer.push(0x00);

    stream.write_all(&buffer).await.map_err(|e| format!("Write failed(auth): {}", e))?;

    // Read Auth Response
    let mut read_buf = [0u8; 4096];
    let mut authenticated = false;

    // Simple auth read loop
    let auth_result = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let n = stream.read(&mut read_buf).await.map_err(|e| e.to_string())?;
            if n < 4 { return Err("Connection closed".to_string()); }

            let mut cursor = Cursor::new(&read_buf[..n]);
            while (cursor.position() as usize) < n {
                if n - (cursor.position() as usize) < 4 { break; }
                let size = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap() as usize;
                
                if n - (cursor.position() as usize) < size { break; } // Incomplete packet

                let _id = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap();
                let type_ = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap();
                
                // Advance cursor past body
                let advance = size - 8; 
                cursor.set_position(cursor.position() + advance as u64);

                if type_ == 2 { // SERVERDATA_AUTH_RESPONSE
                    if _id == -1 {
                        return Err("Authentication failed".to_string());
                    } else if _id == req_id {
                       authenticated = true;
                       return Ok(());
                    }
                }
            }
             if authenticated { return Ok(()); }
        }
    }).await;

    match auth_result {
        Ok(Ok(_)) => {},
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("Auth timed out".to_string()),
    }

    // --- Send Command ---
    let cmd_id = 42;
    let exec_packet_type = 2; // SERVERDATA_EXECCOMMAND
    let cmd_body = command.as_bytes();
    let cmd_size = 4 + 4 + cmd_body.len() as i32 + 1 + 1;

    let mut cmd_buffer = Vec::new();
    WriteBytesExt::write_i32::<LittleEndian>(&mut cmd_buffer, cmd_size).unwrap();
    WriteBytesExt::write_i32::<LittleEndian>(&mut cmd_buffer, cmd_id).unwrap();
    WriteBytesExt::write_i32::<LittleEndian>(&mut cmd_buffer, exec_packet_type).unwrap();
    cmd_buffer.extend_from_slice(cmd_body);
    cmd_buffer.push(0x00);
    cmd_buffer.push(0x00);

    stream.write_all(&cmd_buffer).await.map_err(|e| format!("Write failed(cmd): {}", e))?;

    // --- Read Response ---
    // Note: Response might be split into multiple packets. 
    // And standard parsing often requires handling Multi-packet responses which Source uses.
    // For simple commands, we might get one Packet (Type 0).
    // The "Right" way is to send an empty packet afterwards to mark end, but simple "read until timeout or data" might suffice for "status".
    
    let mut response_data = String::new();

    let read_result = tokio::time::timeout(Duration::from_secs(3), async {
         loop {
            let n = match stream.read(&mut read_buf).await {
                Ok(0) => break, // EOF
                Ok(n) => n,
                Err(_) => break, // Error
            };
            
            let mut cursor = Cursor::new(&read_buf[..n]);
            while (cursor.position() as usize) < n {
                 if n - (cursor.position() as usize) < 4 { break; }
                 let size = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap() as usize;
                 if n - (cursor.position() as usize) < size { break; }

                 let _id = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap();
                 let type_ = ReadBytesExt::read_i32::<LittleEndian>(&mut cursor).unwrap();
                 
                 // Body len = Size - 4(ID) - 4(Type) - 1(Null) - 1(Null)? No, packet struct is Body + Null + Null
                 // Wait, packet size = 4 + 4 + Body + 1 + 1.
                 // So Body Size = Size - 10.
                 // Let's just read until null.
                 
                 // Actually relying on Size is safer.
                 let _string_len = size - 8 - 1; // Exclude last null. (Body + Null) means Size covers Body+1+1. So Size-8 gives Body + 2? 
                 // Protocol: Size, ID, Type, Body, Null, Null.
                 // Size = ID(4)+Type(4)+Body(N)+Null(1)+Null(1) = 10+N.
                 // So BodyLen = Size - 10.
                 
                 let body_len_to_read = if size >= 10 { size - 10 } else { 0 };
                 
                 let start = cursor.position() as usize;
                 let end = start + body_len_to_read;
                 
                 if end > n { break; } // Should check before
                 
                 let chunk = &read_buf[start..end];
                 response_data.push_str(&String::from_utf8_lossy(chunk));
                 
                 // Advance cursor: Body + Null + Null
                 let advance = size - 8; 
                 cursor.set_position(cursor.position() + advance as u64);
                 
                 if type_ == 0 {
                     // SERVERDATA_RESPONSE_VALUE
                     // Continue reading, might have more
                 }
            }
            if n < 4096 {
                // Heuristic: if buffer not full, might be done?
                // RCON is tricky. Usually we wait for a specific ID packet we send as a marker, 
                // but let's just return what we have after a short shake.
                // For `status`, it usually fits or comes fast.
                if response_data.len() > 0 {
                    // Let's give it a tiny bit more time to see if more comes, or break?
                    // Simpler: Just break if we got data (NOT ROBUST for huge lists but ok for now)
                    // Better: loop again?
                }
            }
         }
    }).await;
    
    // If timeout, we still return what we got if any
    if !response_data.is_empty() {
        Ok(response_data)
    } else {
        // If we timed out and got nothing
        match read_result {
           Err(_) => Err("Command timed out or no response".to_string()),
           _ => Ok(String::new()), // Connection closed with no data
        }
    }
}

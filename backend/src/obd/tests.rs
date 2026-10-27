use super::*;
use std::{
    io::Cursor,
    sync::{Arc, Mutex},
};
#[derive(Clone)]
struct ScriptedPort {
    buffer: Cursor<Vec<u8>>,
    writes: Arc<Mutex<Vec<String>>>,
}
impl Read for ScriptedPort {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.buffer.read(buffer)?;
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "emulated timeout",
            ));
        }
        Ok(count)
    }
}
impl Write for ScriptedPort {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let command = String::from_utf8_lossy(bytes).trim().to_string();
        self.writes.lock().unwrap().push(command.clone());
        let reply = match command.as_str() {
            "010C" => "7E8 04 41 0C 1A F8\r>",
            "03" => "7E8 04 43 01 01 00\r>",
            _ => "NO DATA\r>",
        };
        self.buffer = Cursor::new(reply.as_bytes().to_vec());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl SerialPort for ScriptedPort {
    fn name(&self) -> Option<String> {
        Some("ELM327 test emulator".to_string())
    }
    fn baud_rate(&self) -> serialport::Result<u32> {
        Ok(0)
    }
    fn data_bits(&self) -> serialport::Result<serialport::DataBits> {
        Ok(serialport::DataBits::Eight)
    }
    fn flow_control(&self) -> serialport::Result<serialport::FlowControl> {
        Ok(serialport::FlowControl::None)
    }
    fn parity(&self) -> serialport::Result<serialport::Parity> {
        Ok(serialport::Parity::None)
    }
    fn stop_bits(&self) -> serialport::Result<serialport::StopBits> {
        Ok(serialport::StopBits::One)
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(0)
    }
    fn set_timeout(&mut self, _timeout: Duration) -> serialport::Result<()> {
        Ok(())
    }
    fn write_request_to_send(&mut self, _level: bool) -> serialport::Result<()> {
        Ok(())
    }
    fn write_data_terminal_ready(&mut self, _level: bool) -> serialport::Result<()> {
        Ok(())
    }
    fn read_clear_to_send(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn read_data_set_ready(&mut self) -> serialport::Result<bool> {
        Ok(true)
    }
    fn read_ring_indicator(&mut self) -> serialport::Result<bool> {
        Ok(false)
    }
    fn read_carrier_detect(&mut self) -> serialport::Result<bool> {
        Ok(false)
    }
    fn bytes_to_read(&self) -> serialport::Result<u32> {
        Ok(0)
    }
    fn bytes_to_write(&self) -> serialport::Result<u32> {
        Ok(0)
    }
    fn clear(&self, _buffer: serialport::ClearBuffer) -> serialport::Result<()> {
        Ok(())
    }
    fn try_clone(&self) -> serialport::Result<Box<dyn SerialPort>> {
        Ok(Box::new(self.clone()))
    }
    fn set_baud_rate(&mut self, _baud_rate: u32) -> serialport::Result<()> {
        Ok(())
    }
    fn set_data_bits(&mut self, _data_bits: serialport::DataBits) -> serialport::Result<()> {
        Ok(())
    }
    fn set_flow_control(
        &mut self,
        _flow_control: serialport::FlowControl,
    ) -> serialport::Result<()> {
        Ok(())
    }
    fn set_parity(&mut self, _parity: serialport::Parity) -> serialport::Result<()> {
        Ok(())
    }
    fn set_stop_bits(&mut self, _stop_bits: serialport::StopBits) -> serialport::Result<()> {
        Ok(())
    }
    fn set_break(&self) -> serialport::Result<()> {
        Ok(())
    }
    fn clear_break(&self) -> serialport::Result<()> {
        Ok(())
    }
}

#[test]
fn elm327_serial_emulator_preserves_rpm_codes_and_scan_failure() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let mut obd = OBD::new();
    obd.connection = Some(Box::new(ScriptedPort {
        buffer: Cursor::new(Vec::new()),
        writes: writes.clone(),
    }));
    assert_eq!(obd.rpm().value, 1726.0);
    let codes = obd.scan_trouble_codes().unwrap();
    assert_eq!(codes.len(), 1);
    assert_eq!(codes[0].dtc, "P0100");
    assert!(obd.scan_permanent_trouble_codes().is_err());
    assert_eq!(*writes.lock().unwrap(), vec!["010C", "03", "0A"]);
}

use serde::{Deserialize, Serialize};
use sqlite::State;
use std::{default, fmt};

use crate::{
    engine::EngineType,
    scalar::{Scalar, Unit},
    Command, Error, CODE_DESC_DB_PATH, OBD,
};

#[derive(Debug)]
pub enum OBDStandard {
    Standard(&'static str),
}

impl OBDStandard {
    pub fn from_u8(num: u8) -> Self {
        match num {
            1u8 => Self::Standard("OBD-II as defined by CARB"),
            2u8 => Self::Standard("OBD as defined by the EPA"),
            3u8 => Self::Standard("OBD and OBD-II"),
            4u8 => Self::Standard("OBD-I"),
            5u8 => Self::Standard("Not OBD compliant"),
            6u8 => Self::Standard("EOBD"),
            7u8 => Self::Standard("EOBD and OBD-II"),
            8u8 => Self::Standard("EOBD and OBD"),
            9u8 => Self::Standard("EOBD, OBD and OBD-II"),
            10u8 => Self::Standard("JOBD"),
            11u8 => Self::Standard("JOBD and OBD-II"),
            12u8 => Self::Standard("JOBD and EOBD"),
            13u8 => Self::Standard("JOBD, EOBD and OBD-II"),
            // 14-16: Reserved
            17u8 => Self::Standard("Engine Manufacturer Diagnostics"),
            18u8 => Self::Standard("Engine Manufacturer Diagnostics Enhanced"),
            19u8 => Self::Standard("Heavy Duty On-Board Diagnostics (Child/Partial)"),
            20u8 => Self::Standard("Heavy Duty On-Board Diagnostics"),
            21u8 => Self::Standard("World Wide Harmonized OBD"),
            // 22: Reserved
            23u8 => Self::Standard("Heavy Duty Euro OBD Stage I without NOx control"),
            24u8 => Self::Standard("Heavy Duty Euro OBD Stage I with NOx control"),
            25u8 => Self::Standard("Heavy Duty Euro OBD Stage II without NOx control"),
            26u8 => Self::Standard("Heavy Duty Euro OBD Stage II with NOx control"),
            // 27: Reserved
            28u8 => Self::Standard("Brazil OBD Phase 1"),
            29u8 => Self::Standard("Brazil OBD Phase 2"),
            30u8 => Self::Standard("Korean OBD"),
            31u8 => Self::Standard("India OBD I"),
            32u8 => Self::Standard("India OBD II"),
            33u8 => Self::Standard("Heavy Duty Euro OBD Stage VI"),
            // 34-250: Reserved
            // 251-255: Unavailable
            _ => Self::Standard("No data"),
        }
    }
}

impl fmt::Display for OBDStandard {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OBDStandard::Standard(value) => write!(f, "{}", value),
        }
    }
}

#[derive(Debug)]
pub enum AuxiliaryInputStatus {
    InUse,
    NotInUse,
}

impl AuxiliaryInputStatus {
    pub fn as_str(&self) -> &str {
        match self {
            AuxiliaryInputStatus::InUse => "Active",
            AuxiliaryInputStatus::NotInUse => "Inactive",
        }
    }
}

impl fmt::Display for AuxiliaryInputStatus {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum TroubleCodeCategory {
    Powertrain,
    Chassis,
    Body,
    Network,
    #[default]
    Unknown,
}

impl TroubleCodeCategory {
    pub fn system_letter(&self) -> char {
        match self {
            TroubleCodeCategory::Powertrain => 'P',
            TroubleCodeCategory::Chassis => 'C',
            TroubleCodeCategory::Body => 'B',
            TroubleCodeCategory::Network => 'U',
            TroubleCodeCategory::Unknown => '?',
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            TroubleCodeCategory::Powertrain => "Powertrain",
            TroubleCodeCategory::Chassis => "Chassis",
            TroubleCodeCategory::Body => "Body",
            TroubleCodeCategory::Network => "Network",
            TroubleCodeCategory::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for TroubleCodeCategory {
    #[inline(always)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Default, Clone)]
pub struct TroubleCode {
    pub category: TroubleCodeCategory,
    pub dtc: String,
    pub description: String,

    /// Is this a permanant OBD code or a pending one ?
    pub permanant: bool,
}

impl TroubleCode {
    pub fn new(category: TroubleCodeCategory, dtc: String, permanant: bool) -> Self {
        let mut code = Self {
            category,
            dtc,
            description: String::default(),
            permanant,
        };

        code.set_description();
        code
    }

    pub fn set_description(&mut self) {
        self.description = "none".to_string(); // default

        // connect to trouble code data base
        let con = match sqlite::Connection::open(CODE_DESC_DB_PATH) {
            Ok(con) => con,
            Err(err) => {
                println!("when connecting to codes database: {err}");
                return;
            }
        };

        // query description for 'dtc'
        let query = "SELECT desc FROM codes WHERE id = ?";
        let mut statement = match con.prepare(query) {
            Ok(statement) => statement,
            Err(err) => {
                println!("when sanitizing statement {query}: {err}");
                return;
            }
        };

        match statement.bind((1, self.dtc.as_str())) {
            Ok(_) => {}
            Err(err) => {
                println!("when binding dtc '{}' to query {query}: {err}", self.dtc);
                return;
            }
        };

        if let Ok(State::Row) = statement.next() {
            self.description = statement.read::<String, _>("desc").unwrap_or_default();
        }
    }
}

impl fmt::Display for TroubleCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Showing details for trouble code {}:", self.dtc)?;
        writeln!(f, "Trouble code: {}", self.dtc)?;
        writeln!(f, "System letter: {}", self.category.system_letter())?;
        writeln!(f, "Location: {}", self.category)?;
        writeln!(f, "Description: {}", self.description)?;
        write!(f, "Overview: {} is an issue related to {}. This code is from the {} component of the vehicle.", self.dtc, self.description, self.category)
    }
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Test {
    pub name: &'static str,
    pub available: bool,
    pub complete: bool,
}

impl fmt::Display for Test {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Test name: {}", self.name).unwrap();
        writeln!(
            f,
            "Available: {}",
            if self.available { "Yes" } else { "No" }
        )
        .unwrap();
        write!(f, "Complete: {}", if self.complete { "Yes" } else { "No" })
    }
}

impl Test {
    pub fn new(name: &'static str, available: bool, complete: bool) -> Self {
        Self {
            name,
            available,
            complete,
        }
    }

    fn no_data() -> Self {
        Self {
            name: "Unknown",
            ..Default::default()
        }
    }
}

impl OBD {
    /// Fallible scan used by recorded diagnostic evidence. Missing responses are never an empty success.
    pub fn scan_trouble_codes(&mut self) -> Result<Vec<TroubleCode>, Error> {
        self.scan_codes(b"03", "43")
    }
    pub fn scan_permanent_trouble_codes(&mut self) -> Result<Vec<TroubleCode>, Error> {
        self.scan_codes(b"0A", "4A")
    }
    fn scan_codes(&mut self, command: &[u8; 2], positive: &str) -> Result<Vec<TroubleCode>, Error> {
        if !self.is_connected() {
            return Err(Error::NoConnection);
        }
        let mut request = Command::new_svc(command);
        self.send_command(&mut request)?;
        let raw = if self.replay_requests {
            self.get_recorded_response(&request)
                .raw_response()
                .ok_or(Error::NoData)?
        } else {
            self.read_until(b'>')?
        };
        if self.record_requests {
            self.save_request(&request, &crate::Response::new(raw.clone(), raw.clone()));
        }
        let service = u8::from_str_radix(positive, 16).map_err(|_| Error::InvalidResponse)?;
        super::dtc::decode_response(&raw, service)
    }
    /// Get the DTC that caused the freeze frame.
    pub fn get_freeze_frame_dtc(&mut self) -> Vec<TroubleCode> {
        if let Err(err) = self.send_command(&mut Command::new_pid(b"0102")) {
            println!("when getting dtc3: {err}");
            return Vec::new();
        }

        let mut raw_response = self.read_iso_tp_response().join("");
        OBD::decode_trouble_codes(&mut raw_response)
    }

    pub fn get_permanant_trouble_codes(&mut self) -> Vec<TroubleCode> {
        if let Err(err) = self.send_command(&mut Command::new_svc(b"0A")) {
            println!("when getting dtc6: {err}");
            return Vec::new();
        }

        let mut raw_response = self.read_iso_tp_response().join("");
        OBD::decode_trouble_codes(&mut raw_response)
    }

    pub fn clear_trouble_codes(&mut self) -> Result<(), Error> {
        let response = self.query(Command::new_svc(b"04"));
        let raw = response.formatted_response.unwrap_or_default();

        // positive response
        if raw == "44" {
            Ok(())
        } else {
            Err(Error::DTCClearFailed(raw))
        }
    }

    // See https://en.wikipedia.org/wiki/OBD-II_PIDs#Service_01_PID_01
    //
    // Common tests incldue 'components, fuel system, misfire'"
    // If a response is invalid, Tests will have the name "Unknown"
    pub fn get_common_tests_status(&mut self) -> [Test; 3] {
        let response = self.query(Command::new_pid(b"0101"));
        if *response.get_payload_size() == 0 {
            return [Test::no_data(); 3];
        }

        let byte = response.a_value() as u32;

        // "For bits indicating test availability a bit set to 1
        // indicates available, whilst for bits indicating test completeness
        // a bit set to 0 indicates complete. "

        let components = Test::new(
            "Components",
            (byte & 0b0000_0100) != 0,
            // '== 0' instead of '!= 0'
            // complete will be true if bit is equal to 0
            (byte & 0b0100_0000) == 0,
        );

        let fuel_system = Test::new(
            "Fuel System",
            (byte & 0b0000_0010) != 0,
            (byte & 0b0010_0000) == 0,
        );

        let misfire = Test::new(
            "Misfire",
            (byte & 0b0000_0001) != 0,
            (byte & 0b0001_0000) == 0,
        );

        [components, fuel_system, misfire]
    }

    // These tests are engine-type specific
    // Tests will differ if the engine type
    // is compression vs. spark ignition
    //
    // For Compression engines, two tests are reserved.
    // If a response is invalid, Tests will have the name "Unknown"
    pub fn get_advanced_tests_status(&mut self) -> [Test; 8] {
        let engine_type = self.get_engine_type();
        let response = self.query(Command::new_pid(b"0101"));
        if *response.get_payload_size() == 0 {
            return [Test::no_data(); 8];
        }

        let c_byte = response.c_value() as u32;
        let d_byte = response.d_value() as u32;

        let mut tests: [Test; 8] = [Test::no_data(); 8];
        for (index, test) in tests.iter_mut().enumerate() {
            let bit = 1 << (7 - index);
            let available = (c_byte & bit) != 0;

            // Complete will only be able to be true if the test is available
            let complete = if available {
                (d_byte & bit) == 0
            } else {
                false
            };

            let name = match engine_type {
                EngineType::SparkIgnition => match index + 1 {
                    1 => "EGR and/or VVT System",
                    2 => "Oxygen Sensor Heater",
                    3 => "Oxygen Sensor",
                    4 => "Gasoline Particulate Filter",
                    5 => "Secondary Air System",
                    6 => "Evaporative System",
                    7 => "Heated Catalyst",
                    8 => "Catalyst",
                    _ => unreachable!(),
                },
                EngineType::CompressionIgnition => match index + 1 {
                    1 => "EGR and/or VVT System",
                    2 => "PM filter monitoring",
                    3 => "Exhaust Gas Sensor",
                    4 | 6 => "Reserved",
                    5 => "Boost Pressure",
                    7 => "NOx/SCR Monitor ",
                    8 => "NMHC Catalyst",
                    _ => unreachable!(),
                },
                _ => "Unknown",
            };

            *test = Test::new(name, available, complete);
        }

        tests
    }

    pub fn get_test_results(&mut self) {
        // TODO. Test and get a mode 06 response
        if let Err(err) = self.send_command(&mut Command::new_svc(b"06")) {
            println!("when sending test results. mode 06: {err}");
        }

        match self.read_until(b'>') {
            Ok(raw_response) => {
                println!("< {}", raw_response.escape_default());
            }
            Err(err) => {
                println!("when getting test results. mode 06: {err}");
            }
        }
    }

    pub fn has_check_engine_light(&mut self) -> bool {
        let response = self.query(Command::new_pid(b"0101"));
        (response.a_value() as u32 & 0x80) != 0
    }

    pub fn warm_ups_since_codes_cleared(&mut self) -> Scalar {
        self.query(Command::new_pid(b"0130"))
            .map_no_data(|r| Scalar::new(r.a_value(), Unit::NoData, Some(self.unit_preferences)))
    }

    pub fn distance_traveled_since_codes_cleared(&mut self) -> Scalar {
        self.query(Command::new_pid(b"0131")).map_no_data(|r| {
            Scalar::new(
                (256.0 * r.a_value()) + r.b_value(),
                Unit::Kilometers,
                Some(self.unit_preferences),
            )
        })
    }

    pub fn get_num_trouble_codes(&mut self) -> u32 {
        let response = self.query(Command::new_pid(b"0101"));
        response.a_value() as u32 & 0x7F
    }

    // Check on this. Might be broken when there are more than 3 DTC's
    pub fn get_trouble_codes(&mut self) -> Vec<TroubleCode> {
        let n_dtcs = self.get_num_trouble_codes();
        if n_dtcs == 0 {
            // no trouble codes
            return Vec::new();
        }

        if let Err(err) = self.send_command(&mut Command::new_svc(b"03")) {
            println!("when getting dtc1: {err}");
            return Vec::new();
        }

        let mut raw_response = self.read_iso_tp_response().join("");
        OBD::decode_trouble_codes(&mut raw_response)
    }

    pub fn decode_trouble_codes(response: &mut String) -> Vec<TroubleCode> {
        super::dtc::decode_response(response, 0x43)
            .or_else(|_| super::dtc::decode_response(response, 0x4A))
            .unwrap_or_default()
    }
    pub fn distance_traveled_with_mil(&mut self) -> Scalar {
        self.query(Command::new_pid(b"0121")).map_no_data(|r| {
            Scalar::new(
                (256.0 * r.a_value()) + r.b_value(),
                Unit::Kilometers,
                Some(self.unit_preferences),
            )
        })
    }

    pub fn time_run_with_mil(&mut self) -> Scalar {
        self.query(Command::new_pid(b"014D")).map_no_data(|r| {
            Scalar::new(
                (256.0 * r.a_value()) + r.b_value(),
                Unit::Minutes,
                Some(self.unit_preferences),
            )
        })
    }

    pub fn control_module_voltage(&mut self) -> Scalar {
        self.query(Command::new_pid(b"0142")).map_no_data(|r| {
            Scalar::new(
                ((256.0 * r.a_value()) + r.b_value()) / 1000.0,
                Unit::Volts,
                Some(self.unit_preferences),
            )
        })
    }

    pub fn time_since_codes_cleared(&mut self) -> Scalar {
        self.query(Command::new_pid(b"014E")).map_no_data(|r| {
            Scalar::new(
                (256.0 * r.a_value()) + r.b_value(),
                Unit::Minutes,
                Some(self.unit_preferences),
            )
        })
    }
}

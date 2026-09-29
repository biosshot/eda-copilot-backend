// PortableScope 200 MSPS / 14-bit / one channel. Source: EasyEDA Board1, PCB1.
// The eight installed mechanical parts and the board outline come from existing-placement.json.
board.rect(120, 60.833, { layers: ["top", "bottom"], defaultLayer: "bottom", clearance: 0.35, edge: 0.5 });
preserve({ board: true, components: ["U6", "SW1", "SW2", "SW3", "SW4", "SW5", "J4", "RF1"] });
solver({ grid: 1, ignoredSignals: ["GND"], compactness: "normal" });

// Front-panel hardware and the electrical circuits that must meet it.
block("display", ["U6"], "connector");
block("keys", ["SW1", "SW2", "SW3", "SW4"], "connector", { allowDisconnected: true });
block("usb_input", ["J4", "D1", "C43", "R14", "R15"], "connector");
block("rf_port", ["RF1"], "connector");
block("power_switch", ["SW5", "R20"], "power");
component("U6").role("connector").top().boardOverflow({ left: 50, top: 20, bottom: 20 });
component("RF1").role("connector").top().boardOverflow({ right: 5 });
component("J4").role("connector").bottom().boardOverflow({ left: 4 });
component("SW5").role("connector").bottom();
for (const key of ["SW1", "SW2", "SW3", "SW4"]) component(key).role("connector").top();

// MCU controls the display and front panel; its VCAP and local supply parts stay nearby.
block("mcu_core", ["U5", "C34", "C35", "R12"], "mcu");
block("mcu_supply", ["C36", "C37", "C38", "C39", "C40", "C41", "C42"], "mcu", {
  placement: "satellite", attachTo: "mcu_core", anchor: pin("U5", "100")
});
block("mcu_debug", ["J2", "R13"], "connector");
component("U5").role("main_ic");
component("J2").role("connector");
veryNear(pin("C34", "1"), pin("U5", "48"), "high");
veryNear(pin("C35", "1"), pin("U5", "73"), "high");
near(comp("U5"), comp("U6"), "normal");
near(comp("J2"), comp("U5"), "normal");

// ADC and 200 MHz sampling clock. Differential clock legs each retain their own series capacitor.
block("adc_core", ["U1", "R1", "R2", "C9"], "analog");
block("adc_analog_supply", ["C1", "C2", "C3", "C4", "C5", "C6", "C10"], "analog", {
  placement: "satellite", attachTo: "adc_core", anchor: pin("U1", "18")
});
block("adc_digital_supply", ["C7", "C8"], "analog", {
  placement: "satellite", attachTo: "adc_core", anchor: pin("U1", "35")
});
block("adc_clock", ["U2", "C11", "C12", "C13", "C14", "R3", "R4"], "analog");
component("U1").role("main_ic");
component("U2").role("main_ic");
for (const analog of ["R1", "R2", "C9", "C1", "C2", "C3", "C4", "C5", "C6", "C7", "C8", "C10", "C11", "C12", "C13", "C14", "R3", "R4", "R60", "R61", "R62", "R63", "R64", "C95", "C96", "C97", "C98", "C122", "R73", "R74", "C100", "FB1", "FB2", "FB3", "C101", "C102", "C103", "C104", "C105", "C106", "C107", "C108", "C109"]) component(analog).bottom();
component("U1").bottom();
component("U2").bottom();
veryNear(pin("C9", "1"), pin("U1", "13"), "high");
signalPath("sample_clock_p", [[pin("U2", "4"), pin("C11", "1")], [pin("C11", "2"), pin("U1", "10")]], { priority: "high" });
signalPath("sample_clock_m", [[pin("U2", "5"), pin("C12", "1")], [pin("C12", "2"), pin("U1", "11")]], { priority: "high" });
near(comp("U1"), comp("U3"), "high");

// RF connector, compensated attenuator, protected range select, buffer and differential ADC driver.
block("afe_attenuator", ["R60", "R61", "R62", "R63", "R64", "C95", "C96", "C97", "C98", "C122"], "rf");
block("afe_protected_range", ["U20", "U21", "U22", "U23", "U24", "R66"], "analog");
block("afe_ac_buffer", ["U25", "U26", "C99", "R65", "R67"], "analog");
block("afe_fda", ["U27", "R68", "R69", "R70", "R71", "R72"], "analog");
block("afe_adc_drive", ["R73", "R74", "C100"], "analog", {
  placement: "satellite", attachTo: "adc_core", anchor: pin("U1", "15")
});
block("afe_positive_rail", ["FB1", "C101", "C102", "C107"], "power", {
  placement: "satellite", attachTo: "afe_protected_range", anchor: pin("U20", "5")
});
block("afe_negative_rail", ["FB2", "C103", "C104", "C108"], "power", {
  placement: "satellite", attachTo: "afe_protected_range", anchor: pin("U20", "6")
});
block("afe_3v3_rail", ["FB3", "C105", "C106", "C109"], "power", {
  placement: "satellite", attachTo: "afe_fda", anchor: pin("U27", "5")
});
for (const ic of ["U20", "U21", "U22", "U23", "U24", "U25", "U26", "U27"]) component(ic).role("main_ic").bottom();
for (const passive of ["R66", "C99", "R65", "R67", "R68", "R69", "R70", "R71", "R72"]) component(passive).bottom();
near(comp("RF1"), comp("R60"), "high");
near(comp("R61"), comp("U21"), "high");
signalPath("direct_range_to_adc", [
  [pin("RF1", "2"), pin("U20", "1")],
  [pin("U20", "9"), pin("U24", "4")],
  [pin("U24", "6"), pin("U25", "7")],
  [pin("U25", "6"), pin("U26", "4")],
  [pin("U26", "7"), pin("R67", "1")],
  [pin("R67", "2"), pin("U27", "2")],
  [pin("U27", "10"), pin("R73", "1")],
  [pin("R73", "2"), pin("R1", "1")],
  [pin("R1", "2"), pin("U1", "15")],
], { priority: "high", shape: "flexible" });
signalPath("adc_driver_negative", [
  [pin("U27", "11"), pin("R74", "1")],
  [pin("R74", "2"), pin("R2", "1")],
  [pin("R2", "2"), pin("U1", "16")],
], { priority: "high" });

// Battery charging and conversion. Switch-node pairs are the dominant local connections.
block("charger", ["U7", "J5", "C44", "C45", "R16", "R17", "R18", "R19"], "power");
block("system_boost", ["U8", "L1", "C46", "C47", "C48", "R21"], "power");
block("system_3v3_bulk", ["C49", "C50", "C51", "C52"], "power", {
  placement: "satellite", attachTo: "system_boost", anchor: pin("U8", "3")
});
block("fpga_1v1_reg", ["U9", "L2", "C53", "C54", "C55", "C66", "R22", "R23", "R24"], "power");
block("fpga_1v8_reg", ["U10", "L3", "C56", "C57", "C58", "R25"], "power");
block("fpga_2v5_reg", ["U11", "C59", "C60"], "power");
block("adc_analog_reg", ["U12", "C61", "C62"], "power");
block("adc_digital_reg", ["U13", "C63", "C64"], "power");
block("clock_reg", ["U14", "C65"], "power");
block("ddr_1v5_reg", ["U15", "L4", "C67", "C68", "C69", "R26", "R27", "R28"], "power");
block("ddr_termination_reg", ["U16", "C70", "C71", "C72", "C73", "C74", "C75", "C76", "C77", "R29", "R30", "R31"], "power");
block("analog_bipolar_reg", ["U18", "L5", "L6", "D2", "D3", "C110", "C111", "C112", "R79"], "power");
block("analog_positive_bulk", ["C113", "C114", "C115"], "power", {
  placement: "satellite", attachTo: "analog_bipolar_reg", anchor: pin("U18", "23")
});
block("analog_negative_bulk", ["C116", "C117", "C118"], "power", {
  placement: "satellite", attachTo: "analog_bipolar_reg", anchor: pin("U18", "15")
});
block("analog_bipolar_feedback", ["R75", "R76", "R77", "R78", "C119", "C120", "C121"], "power", {
  placement: "satellite", attachTo: "analog_bipolar_reg", anchor: pin("U18", "16"), allowDisconnected: true
});
component("J5").role("connector");
for (const ic of ["U7", "U8", "U9", "U10", "U11", "U12", "U13", "U14", "U15", "U16", "U18"]) component(ic).role("main_ic");
near(comp("J4"), comp("U7"), "high");
near(comp("SW5"), comp("U8"), "normal");
criticalPair(pin("U8", "8"), pin("L1", "1"), { priority: "high", maxDistance: 4 });
criticalPair(pin("U9", "5"), pin("L2", "1"), { priority: "high", maxDistance: 4 });
criticalPair(pin("U10", "5"), pin("L3", "1"), { priority: "high", maxDistance: 4 });
criticalPair(pin("U15", "5"), pin("L4", "1"), { priority: "high", maxDistance: 4 });
near(comp("U12"), comp("U1"), "high");
near(comp("U13"), comp("U1"), "normal");
near(comp("U14"), comp("U2"), "high");
near(comp("U18"), comp("U20"), "normal");

// FPGA, configuration flash, high-density decoupling, and external DDR.
block("fpga_core", ["U3"], "mcu");
block("fpga_programming", ["J1"], "connector");
block("fpga_flash", ["U4", "C33", "R5", "R6", "R7", "R8", "R9", "R10", "R11"], "mcu");
block("fpga_1v1_caps", ["C15", "C16", "C17", "C18", "C19", "C29"], "mcu", {
  placement: "satellite", attachTo: "fpga_core", anchor: pin("U3", "L14")
});
block("fpga_2v5_caps", ["C20", "C21", "C22", "C30"], "mcu", {
  placement: "satellite", attachTo: "fpga_core"
});
block("fpga_1v8_caps", ["C23", "C24", "C31"], "mcu", {
  placement: "satellite", attachTo: "fpga_core"
});
block("fpga_3v3_caps", ["C25", "C26", "C27", "C28", "C32"], "mcu", {
  placement: "satellite", attachTo: "fpga_core"
});
block("ddr_core", ["U17", "R32", "R33", "R34", "R35", "C93", "C94"], "mcu");
block("ddr_1v5_caps_a", ["C79", "C80", "C81", "C82", "C83", "C84", "C85", "C86"], "mcu", {
  placement: "satellite", attachTo: "ddr_core", anchor: pin("U17", "R9")
});
block("ddr_1v5_caps_b", ["C87", "C88", "C89", "C90", "C91", "C92"], "mcu", {
  placement: "satellite", attachTo: "ddr_core", anchor: pin("U17", "A1")
});
block("ddr_address_term_low", ["R36", "R37", "R38", "R39", "R40", "R41"], "mcu", {
  placement: "satellite", attachTo: "ddr_core"
});
block("ddr_address_term_high", ["R42", "R43", "R44", "R45", "R46", "R47", "R48", "R49", "R50", "R51"], "mcu", {
  placement: "satellite", attachTo: "ddr_core"
});
block("ddr_control_term", ["R52", "R53", "R54", "R55", "R56", "R57"], "mcu", {
  placement: "satellite", attachTo: "ddr_core"
});
component("U3").role("main_ic");
component("U17").role("main_ic");
component("U4").role("main_ic");
component("J1").role("connector");
near(comp("U17"), comp("U3"), "high");
near(comp("U4"), comp("U3"), "high");
near(comp("J1"), comp("U3"), "normal");
near(comp("U9"), comp("U3"), "normal");
near(comp("U10"), comp("U3"), "normal");
near(comp("U15"), comp("U17"), "normal");
near(comp("U16"), comp("U17"), "high");

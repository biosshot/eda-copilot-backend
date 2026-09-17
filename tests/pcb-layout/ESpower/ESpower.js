// Compact top-side placement for ESPower / Board1 / PCB1.
// Placement only: the board remains two-layer, with all components on TOP.

board.roundedRect(48, 32, {
  radius: 1.5,
  layers: ["top", "bottom"],
  defaultLayer: "top",
  clearance: 0.25,
  edge: 0.45,
});

// Board-level mechanics.
block("mounting_left", ["SCREW1"], "connector", null, { allowDisconnected: true });
block("mounting_right", ["SCREW2"], "connector", null, { allowDisconnected: true });
block("usb_port", ["U12", "R5", "R6", "D1", "U6", "C7", "C8", "C16"], "connector");
block("antenna", ["U11"], "rf", null, { allowDisconnected: true });
block("reset_button", ["U4"], "connector");
block("boot_button", ["U5"], "connector");
block("slide_switch", ["SW2"], "connector", null, { allowDisconnected: true });
block("battery_connector", ["U9"], "connector");
block("sense_connector", ["U8"], "connector");
block("ground_connector", ["U14"], "connector", null, { allowDisconnected: true });

// ESP32-C3 core and local satellites.
block("mcu", ["U1"], "mcu");
block("mcu_decoupling", ["C9", "C10"], "mcu", null, {
  placement: "satellite",
  attachTo: "mcu",
  anchor: pin("U1", "31"),
});
block("mcu_decoupling_1", ["C15"], "mcu", null, {
  placement: "satellite",
  attachTo: "mcu",
  anchor: pin("U1", "11"),
});

block("flash_decoupling", ["C3"], "mcu", null, {
  placement: "satellite",
  attachTo: "mcu",
  anchor: pin("U1", "18"),
});
block("crystal", ["X1", "C4", "C6"], "mcu", null, {
  placement: "satellite",
  attachTo: "mcu",
  anchor: pin("U1", "29"),
});
block("rf_match", ["L1", "C1", "C2"], "rf", null, {
  placement: "satellite",
  attachTo: "mcu",
  anchor: pin("U1", "1"),
  localLayout: {
    "L1": {
      x: 0,
      y: -0.8,
      rotate: 0
    },
    "C1": {
      x: -2,
      y: 0,
      rotate: 90
    },
    "C2": {
      x: 2,
      y: 0,
      rotate: -90
    }
  }
});
block("strap_pullups", ["R12", "R13"], "mcu", null, {
  placement: "satellite",
  attachTo: "mcu",
  anchor: pin("U1", "8"),
  allowDisconnected: true,
});

block("reset_support", ["R1", "C5"], "mcu", null, {
  placement: "satellite",
  attachTo: "reset_button",
  anchor: pin("U4", "2"),
});
block("boot_support", ["R2"], "boot_button", null, {
  placement: "satellite",
  attachTo: "boot_button",
  anchor: pin("U5", "2"),
});

// USB data, CC and input power.
block("usb_data", ["R7", "R8"], "generic", null, {
  placement: "satellite",
  attachTo: "mcu",
  anchor: pin("U1", "25"),
  allowDisconnected: true,
});
// Power, charger, battery switching and measurement.
block("charger", ["U7", "R10", "R11", "LED1", "C12", "R14"], "power");
block("battery_adc", ["Q2", "C13", "R15", "R16", "R17"], "analog");
component("C12").block("charger").role("decoupling_cap").top();
bypass(["C12"], pin("U7", "4"));
component("C11").block("charger").role("decoupling_cap").top();
component("Q1").block("charger").top();

block("current_monitor", ["U13", "R21", "C14"], "analog");
block("current_monito_pull", ["R19", "R20"], "pull", null, {
  placement: "satellite",
  attachTo: "current_monitor",
  anchor: pin("U13", "4"),
  allowDisconnected: true,
});

bypass(["C14"], pin("U13", "5"));
component("C14").block("current_monitor").role("decoupling_cap").top();
component("C15").block("mcu_decoupling_1").role("decoupling_cap").top();

component("C16").block("usb_port").role("decoupling_cap").top();
bypass(["C16", "C8"], pin("U6", "5"));

component("SCREW1").block("mounting_left").role("connector").top().fixed({ x: -21.5, y: -13.5, rotate: 0, layer: "top" });
component("SCREW2").block("mounting_right").role("connector").top().fixed({ x: 21.5, y: -13.5, rotate: 0, layer: "top" });

component("U12").block("usb_port").role("connector").top().edgeMount("bottom", {
  overhang: 1.2,
  face: "outward",
  align: "center",
});
component("U11").block("antenna").role("main_ic").top().edgePlace("top", {
  inset: 0.5,
  face: "any",
  align: "center",
});
component("U4").block("reset_button").role("connector").top().edgePlace("bottom", { inset: 0.8, face: "any", x: -17 });
component("U5").block("boot_button").role("connector").top().edgePlace("top", { inset: 0.8, face: "any", x: 10 });
component("SW2").block("slide_switch").role("connector").top().edgePlace("right", { inset: 0.5, face: "outward", y: 2 });
component("U9").block("battery_connector").role("connector").top().edgePlace("left", { inset: 0.4, face: "outward", y: 7 });
component("U8").block("sense_connector").role("connector").top().edgePlace("right", { inset: 0.4, face: "outward", y: -7 });
component("U14").block("ground_connector").role("connector").top().edgePlace("left", { inset: 0.4, face: "outward", y: -7 });

component("U1").block("mcu").role("main_ic").top();
for (const d of ["C9", "C10"]) component(d).block("mcu_decoupling").role("decoupling_cap").top();
component("C3").block("flash_decoupling").role("decoupling_cap").top();
component("X1").block("crystal").role("crystal").top();
for (const d of ["C4", "C6"]) component(d).block("crystal").role("passive").top();
for (const d of ["L1", "C1", "C2"]) component(d).block("rf_match").role("passive").top();
for (const d of ["R12", "R13"]) component(d).block("strap_pullups").role("passive").top();
for (const d of ["R19", "R20"]) component(d).block("current_monito_pull").role("passive").top();
component("R1").block("reset_support").role("passive").top();
component("C5").block("reset_support").role("decoupling_cap").top();
component("R2").block("boot_support").role("passive").top();
for (const d of ["R7", "R8"]) component(d).block("usb_data").role("passive").top();
for (const d of ["R5", "R6", "D1"]) component(d).block("usb_port").role("passive").top();
component("U6").block("usb_port").role("main_ic").top();
for (const d of ["C7", "C8"]) component(d).block("usb_port").role("decoupling_cap").top();
component("U7").block("charger").role("main_ic").top();
for (const d of ["R10", "R11"]) component(d).block("charger").role("passive").top();
component("LED1").block("charger").role("indicator").top();
component("Q2").block("battery_adc").role("main_ic").top();
component("C13").block("battery_adc").role("decoupling_cap").top();
for (const d of ["R15", "R16", "R17"]) component(d).block("battery_adc").role("passive").top();
component("U13").block("current_monitor").role("main_ic").top();
component("R21").block("current_monitor").role("passive").top();

// Critical ordered paths and local loops.
signalPath("usb_dm", [
  [pin("U12", "A7"), pin("R7", "1"), { maxDistance: 7, preferFacingPads: true }],
  [pin("R7", "2"), pin("U1", "25"), { maxDistance: 8, preferFacingPads: true }],
], { priority: "critical", shape: "flexible" });
signalPath("usb_dp", [
  [pin("U12", "A6"), pin("R8", "1"), { maxDistance: 7, preferFacingPads: true }],
  [pin("R8", "2"), pin("U1", "26"), { maxDistance: 8, preferFacingPads: true }],
], { priority: "critical", shape: "flexible" });
signalPath("rf_feed", [
  [pin("U1", "1"), pin("L1", "1"), { maxDistance: 4, preferFacingPads: true }],
  [pin("L1", "2"), pin("U11", "1"), { maxDistance: 9, preferFacingPads: true }],
], { priority: "critical", shape: "straight" });

criticalPair(pin("U1", "29"), pin("X1", "1"), { maxDistance: 5, preferFacingPads: true });
criticalPair(pin("U1", "30"), pin("X1", "3"), { maxDistance: 5, preferFacingPads: true });
veryNear(pin("C4", "1"), pin("X1", "1"), "critical");
veryNear(pin("C6", "2"), pin("X1", "3"), "critical");
veryNear(pin("C1", "2"), pin("U1", "1"), "critical");
veryNear(pin("C2", "2"), pin("L1", "2"), "critical");
criticalPair(pin("C3", "2"), pin("U1", "18"), { maxDistance: 4.5, preferFacingPads: true });

bypass(["C9", "C10"], pin("U1", "31"));
bypass(["C15"], pin("U1", "11"));

veryNear(pin("R5", "1"), pin("U12", "A5"), "high");
veryNear(pin("R6", "2"), pin("U12", "B5"), "high");
veryNear(pin("D1", "2"), pin("U12", "A9"), "high");
veryNear(pin("C7", "2"), pin("U6", "1"), "critical");

near(comp("U7"), comp("U9"), "high");
near(comp("Q1"), comp("SW2"), "high");
veryNear(pin("C11", "1"), pin("Q1", "3"), "high");
veryNear(pin("R10", "1"), pin("U7", "5"), "critical");
veryNear(pin("R11", "2"), pin("U7", "1"), "high");
near(comp("LED1"), comp("R11"), "high");

coreIsland("current_sense", ["U13", "R21"], {
  pairs: [
    [pin("R21", "1"), pin("U13", "7")],
    [pin("R21", "2"), pin("U13", "8")],
  ],
  maxDistance: 5,
  priority: "critical",
  preferFacingPads: true,
});
near(comp("R21"), comp("U8"), "critical");
near(comp("U13"), comp("U1"), "normal");
near(comp("Q2"), comp("U1"), "high");

silkscreen.designators({ enabled: true, height: 0.9, rotations: [0, 90], margin: 0.18 });
solver({
  grid: 0.25,
  ignoredSignals: ["GND"],
  // compactness: "high",
});

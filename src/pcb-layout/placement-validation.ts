import type { ExplainCircuit } from "#types/circuit.ts";
import type { PlacementRules } from "#types/pcb/layout-rules.ts";
import { validatePlacementRulesForCircuit as validatePlacementRulesForCircuitCore } from "./placement-input.ts";

/**
 * Allow fixed() for any component role without weakening the unrelated
 * mechanical/satellite validation that is still owned by placement-input.ts.
 *
 * The legacy validator's connector-role check is the only obsolete rule. For
 * validation only, fixed components are presented as connector-role; the real
 * rules and component roles are left unchanged for placement.
 */
export function validatePlacementRulesForCircuit(circuit: ExplainCircuit, rules: PlacementRules) {
    const validationRules: PlacementRules = {
        ...rules,
        component_rules: rules.component_rules.map((rule) => rule.fixedPlacement
            ? { ...rule, role: "connector" as const }
            : rule),
    };
    validatePlacementRulesForCircuitCore(circuit, validationRules);
}

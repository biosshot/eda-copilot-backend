import type { ExplainCircuit } from "#types/circuit.ts";
import type { PlacementRules } from "#types/pcb/layout-rules.ts";
import { validatePlacementRulesForCircuit as validatePlacementRulesForCircuitCore } from "./placement-input-core.ts";

export * from "./placement-input-core.ts";

/**
 * Validate placement semantics while allowing fixed() for every component role.
 *
 * The legacy validator still owns the unrelated fixed-placement checks (for
 * example mechanical placement inside satellite blocks).  Its only obsolete
 * rule is role-based, so validate a shallow rule copy in which fixed components
 * are treated as connector-role solely for that legacy check.  The original
 * rules and component roles continue unchanged into placement.
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

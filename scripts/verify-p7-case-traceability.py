"""Cheap structural validation for the P7 case-to-evidence crosswalk."""
import json, pathlib, sys, hashlib, re
root=pathlib.Path(__file__).resolve().parents[1]
manifest=json.loads((root/"tests/fixtures/analysis-p7-closure/manifest.json").read_text(encoding="utf-8"))
trace=manifest.get("caseTraceability")
assert isinstance(trace,dict), "caseTraceability missing"
rows=trace.get("rows")
assert isinstance(rows,list) and len(rows)==trace.get("rowCount"), "rowCount mismatch"
source=root/trace["source"]
assert source.is_file(), f"missing canonical source: {source}"
assert hashlib.sha256(source.read_bytes()).hexdigest()==trace["sourceSha256"], "canonical matrix hash mismatch; refresh crosswalk deliberately"
lines=source.read_text(encoding="utf-8").splitlines()
canonical=[]
section=None
sections={"## Основные случаи":"primary", "## Дополнительные обязательные приёмки":"supplemental", "### Смена опоры":"return-topology"}
for i,line in enumerate(lines,1):
    if line.startswith("#"):
        section=sections.get(line.strip())
        continue
    # Blank lines may separate headerless continuation rows in the same section.
    if section and line.startswith("|") and not line.startswith(("| Случай |", "|---")):
        cells=[x.strip() for x in line.strip("|").split("|")]
        if cells and cells[0]: canonical.append((i,cells[0],section))
for required in ("Blind/buried via", "Разъём/пад (connector/pad transition)", "Дифференциальная пара"):
    assert any(label==required for _,label,_ in canonical), f"missing required continuation case: {required}"
assert len(rows)==trace["rowCount"]==len(canonical), f"canonical row count mismatch: rows={len(rows)} declared={trace['rowCount']} parsed={len(canonical)}"
ids=set(); labels=set()
for row in rows:
    assert row["id"] not in ids, f"duplicate id: {row['id']}"; ids.add(row["id"])
    assert row["case"] not in labels, f"duplicate case label: {row['case']}"; labels.add(row["case"])
    assert row["sourceLine"] > 0 and row["sourceSection"] in {"primary","supplemental","return-topology"}
    assert (row["sourceLine"],row["case"],row["sourceSection"])==canonical[len(ids)-1], f"row differs from canonical matrix: {row['id']}"
    impl=row["implementation"]; acc=row["acceptance"]
    assert impl["status"] in {"unmapped","mapped"}
    assert isinstance(impl["shortControls"],list) and (impl["status"]!="mapped" or impl["code"])
    for reference in (impl.get("code") or [])+impl["shortControls"]:
        path, separator, symbol = reference.partition("#")
        mapped = root / path
        assert mapped.is_file(), f"missing mapped file: {reference}"
        if separator:
            assert symbol and all(re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", part) for part in symbol.split("::")), f"invalid symbol: {reference}"
            text = mapped.read_text(encoding="utf-8")
            for part in symbol.split("::"):
                assert re.search(r"\b(?:mod|fn)\s+" + re.escape(part) + r"\b", text), f"missing symbol {part}: {reference}"
    assert acc["status"] in {"open","accepted","partial","refused"}
    assert isinstance(acc["observables"],list) and isinstance(acc["finalChecks"],list)
    if acc["status"]=="accepted":
        assert acc["reference"] and acc["observables"] and acc["frequencyBandHz"] and acc["finalChecks"], f"accepted without evidence: {row['id']}"
print(f"case traceability structure: PASS ({len(rows)} rows; implementation mappings={sum(r['implementation']['status']=='mapped' for r in rows)})")


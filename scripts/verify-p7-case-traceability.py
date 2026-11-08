"""Cheap structural validation for the P7 case-to-evidence crosswalk."""
import json, pathlib, sys, hashlib
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
for i,line in enumerate(lines,1):
    if line.startswith("| Случай |"):
        table=True
        section={1:"primary",2:"supplemental",3:"return-topology"}.get(sum(1 for x in lines[:i] if x.startswith("| Случай |")))
        continue
    if "table" in locals() and table:
        if not line.startswith("|"):
            table=False
            continue
        if line.startswith("|---"):
            continue
        cells=[x.strip() for x in line.strip("|").split("|")]
        if cells and cells[0]: canonical.append((i,cells[0],section))
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
    for path in (impl.get("code") or [])+impl["shortControls"]: assert (root/path).is_file(), f"missing mapped file: {path}"
    assert acc["status"] in {"open","accepted","partial","refused"}
    assert isinstance(acc["observables"],list) and isinstance(acc["finalChecks"],list)
    if acc["status"]=="accepted":
        assert acc["reference"] and acc["observables"] and acc["frequencyBandHz"] and acc["finalChecks"], f"accepted without evidence: {row['id']}"
print(f"case traceability structure: PASS ({len(rows)} rows; implementation mappings={sum(r['implementation']['status']=='mapped' for r in rows)})")


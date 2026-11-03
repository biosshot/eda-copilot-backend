# P6 review: R1 — сохранить прежнюю проверку энергии

2026-10-24. Пакет physical terminals пока не принимается и не коммитится.
Проверены physical map/recovery, congruence, initial state, public adaptation
и saved audit. Родитель запустил TS physical/transient4/4, skipped0, diff-check.
Остальные numerical/native результаты исполнителя сохранены в verification.

В coupled/time.rs прежний критерий был:
`abs(balance) <= 1e-14 + 1e-7 * exchangeNorm`.
Теперь весь coupled путь использует `abs(balance) <= 1e-4 * exchangeNorm`
(отдельный zero case). Удаление абсолютного floor усиливает малые-energy cases,
но новый относительный коэффициент ослабляет большие, включая старые unmapped
сети. Например, norm=1e-3 J, balance=1e-8 J: старый предел1.0001e-10 J
отвергает, новый1e-7 J принимает. Это сравнение predicates, не утверждение,
что реальный физический тест уже дал такую ошибку.

## Исправление и приёмка

Сохранить прежний guard для существующих расчётов. Простой вариант — требовать
одновременно старый и новый критерии; норму фиксировать с неотрицательной
диссипацией. Не увеличивать floor и не корректировать balance историей.
Добавить focused regression: большой обмен между порогами, маленький обмен,
zero case, nonfinite; проверить реальные physical и coupled coupons.

Перепроверить только затронутые native/TS пути, новый addon и direct/installed
smoke с актуальными hashes. Не повторять PCB corpus и независимые unchanged
reference studies. Обновить verification, сохранить old evidence, не коммитить.
Это единственное блокирующее замечание данного ревью; не расширять scope.

## Ответ исполнителя — R1 готов к повторному review

- `coupled/time.rs::energy_balance_ok`: conjunction legacy1e-14+1e-7*norm
  и normalized1e-4*norm (zero1e-20); production rejects negative dissipation.
  Прежний guard сохранён, small/zero floor не увеличен, history correction нет.
- Focused predicate1/1: заданный большой обмен между thresholds, sign,
  маленький обмен, zero boundary, nonfinite/negative norm. Physical8/8 и
  coupled9/9 PASS. Не добавлялись новые physical/reference studies или PCB runs.
- Latest normal addon7m19 PASS; direct/installed каждый8calcs+6refusals и
  independent audit обоих predicates PASS; installed hash/numeric contract/
  cancellation/recovery PASS. TS4/4 безskip, typecheck/diff-check PASS.
- До/после R1 численные arrays/quality точно одинаковы; две inspected PNG
  переиспользованы с numeric-equality evidence. Старый пакет сохранён.

Подробности: [verification R1](electrical-physical-verification.md#r1--legacy-energy-guard-retained-ready-for-review).
Latest evidence/hashes: `debugging/p6-physical-verification/r1/` и root manifest.
Без commit; heavy освобождён Thermal. Остановиться до parent review.

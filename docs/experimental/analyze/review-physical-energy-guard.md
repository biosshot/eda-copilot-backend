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

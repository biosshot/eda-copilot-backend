# Thermal: bounded blocked triangular solve prototype

Основание: thermal-transient-performance-proposal.md. Профиль показывает triangular solves46.55%, operator18.29%; factor construction2.45%. Исполнитель thermal_medium; это одобрение малого прототипа, не роста бюджетов и не whole-P9 acceptance.

## Работа

На прежнем faithful10mm Cu/resin/via/package/event coupon сравнить simplicial baseline с blocked/supernodal решением того же SPD preconditioner. Сначала найти существующий путь в pinned faer. Не создавать другой физический operator, менять tolerance/time controller, сетку или policy7 spatial solves. Не возвращать отвергнутый single-slot cache.

Сохранить одинаковые factor/RHS sequences; разделить symbolic/packing/numeric construction/solve/permutation. Проверить residual относительно исходного matrix, обе triangular operations, multiple RHS, крайние размеры и conditioning. Нельзя сравнивать только kernel wall, забыв packing и память.

Plan immutable/System-owned, scratch per-call; учесть padding, symbolic/numeric и replacement peak, все resident slots. Invalidation tests для dt/capacity/tangent/free-order/sparsity/boundary/barrel. Нет reuse между разными meshes по одному board ID. Производственные limits неизменны.

## Gates

Сначала standalone tests и coupon A/B: прежние физические residual/energy/time/spatial gates, original full-field сравнение, отдельно новые differences от порядка сложения. Gate скорости задаётся до запуска, полезный результат требует улучшения полного coupon wall, а не только микрокернела; измерить variance ограниченными повторениями короткого replay (максимум3 на вариант). Planned и observed memory peak не ухудшаются относительно baseline на том же случае. Не выдавать timing noise за ускорение.

Один полный coupon на вариант <=384MiB/120s; replay <=120s total. Full TPS/Band/controller запусков здесь НЕТ. Если кандидат проходит, предоставить измеренный conservative preflight и предложение сопоставимой ordinary/tighter пары для следующего review; прежние ordinary данные не считаются автоматически совместимыми с новой арифметикой. Если выигрыш не проходит — сохранить отказ, никакого роста ресурсов ради PASS.

Добавить графики kernel/end-to-end/памяти/численных разностей по сохранённым данным. Production выбор не переключать до review; допускается внутренний test-only selector. Изолированный target/jobs1/heavy очередь с P6/P7. Common electrical/TS API и placement не трогать. Без коммита/новых агентов. Отчёт и стоп.

# P7: явное разрешение OS paging (2026-10-30)

`scripts/verify-p7-automatic-supervised.py --allow-os-paging` включает только developer режим Windows. Настройки ОС/pagefile не меняются, out-of-core не добавляется. Без флага прежние defaults: дерево 2048 MiB RSS, native plan 2048 MiB, 300 s, RAM reserve 512 MiB.

В paging режиме `--tree-memory-mib` ограничивает сумму **private committed bytes** наблюдаемого родителя Node и рекурсивных потомков (`psutil.memory_info().private` на Windows, PrivateUsage). Это не RSS, не размер виртуального адресного пространства и не полный системный commit: shared committed allocations не относятся к private sum. RSS записывается отдельно. Native memory admission остаётся прежним оценочным бюджетом. Ограничения опрашиваются каждые 50 ms, поэтому это sampled supervisory guard, а не жёсткая Windows Job Object квота; возможны промежуточные пики.

Preflight требует доступный системный commit >= tree cap + reserve. Во время работы требуется commit headroom >= reserve и private tree <= cap; низкая physical available RAM в этом режиме сама по себе не вызывает остановку. Headroom = Windows `GetPerformanceInfo`: `(CommitLimit - CommitTotal) * PageSize`. Pagefile free space для этого не используется. При недоступной системной/private метрике — явный отказ с причиной, без подмены RSS. Preflight также проверяет доступность private метрики текущего supervisor. Physical RAM остаётся диагностикой. Это не резервирование commit: прочие процессы и динамический pagefile могут менять headroom.

Manifest сохраняет memoryMode, memoryCapMetric, commitMetricSource/Error, preflight headroom, минимальные RAM/commit за проход, максимальные RSS/private bytes и причину отказа. Timeout сохранён; Ctrl+C останавливает наблюдаемое дерево и пишет итоговую причину. Никакой полной физической/P7 сертификации режим не даёт.

Без запуска Node/native доступен smoke:

```powershell
python scripts/verify-p7-automatic-supervised.py --allow-os-paging --preflight-only --native-memory-mib 256 --tree-memory-mib 256 --reserve-mib 1
```

Проверки: 15 inline Python assertions для policy boundaries, отличия private от RSS, отказа без private метрики, настоящего Windows headroom и private текущего маленького Python процесса. Пять mock lifecycle проверок без дочерних процессов: private cap, commit reserve, Ctrl+C, timeout, потеря commit метрики; каждая проверяет kill родителя/потомка и сохранённую причину. Оба smoke preflight (paging и default) прошли: `debugging/p7-channel-plan/automatic-supervised-1793366550625749800/resume-manifest.json` и `automatic-supervised-1793366550803041700/resume-manifest.json`. На момент paging smoke available RAM 4,398,333,952 B, available commit 18,942,021,632 B; это разные метрики. Больших выделений, native builds и field runs не выполнялось.

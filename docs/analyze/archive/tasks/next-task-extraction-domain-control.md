# P7: отдельный контроль внешней границы и совмещённой сетки

2026-10-24. §8.13 принят как диагностическое доказательство, не production.
Nested transition enrichment уменьшил differential error5.308→3.701%, но
full/common8.298% всё ещё FAIL. Региональная энергия сходится с полной до1.34e-13;
flux indicator остаётся эвристикой, не сертификатом ошибки.

## Следующие действия

1. Выполнить один domain-only D32 control с неизменной внутренней baseline
   геометрией/пространством. Проверить сохранение внутренних triangles/zplanes,
   actual symbolic factorization и полный memory plan до solve. Сравнить с
   сохранёнными D8/D16: external full/common/differential ошибки и domain change.
2. По результату выбрать один совмещённый D32 + nested coarse-transition
   enrichment контроль. Не изобретать переменные XY planes только ради старого
   диагностического25k cap: для этих двух probes архитектор явно разрешает
   до35000 nodes /4 million local terms. Это изменение только диагностического
   бюджета; production caps и scope остаются прежними. Memory512MiB и wall60s
   на solve неизменны, actual symbolic admission обязателен.
3. Совмещённый контроль должен сохранять старое FE space и доказанные source
   constraints, foil/gap, материалы и full infinity-reference potentials.
   Выбор дополнительных edges обосновать сохранёнными indicators, записать
   до solve. Сравнивать с обоими предыдущими controls и независимым FastCap.
   Не менять5% accuracy gate, не подменять его last-pair agreement.

Всего не более двух новых solves; baseline не пересчитывать. Если ресурсный
план не проходит — proposal вместо запуска. После двух controls остановиться
на готовом evidence packet: точность/отдельные domain и mesh changes либо
конкретный remaining blocker. Даже PASS coupon не означает готовый P7:
реальные geometries, L conditioning/cost, recovery и публичный DSL ещё открыты.

## Результат

Обновить §8.14 исходного задания, короткую таблицу/PNG и manifest, сохранить
failed evidence. Если coupon даёт обоснованную пригодность метода — предложить
конкретный следующий production шаг с переиспользованием существующего CDT;
новый solver/реализацию всей платы пока не начинать. Не коммитить до review.
Слот согласовать с P6/Thermal; короткие сообщения только по итогам/блокерам.

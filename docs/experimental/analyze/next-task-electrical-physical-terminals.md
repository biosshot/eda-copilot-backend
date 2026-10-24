# P6: физические контакты при соединении infinity-reference моделей

2026-10-24. Mixed delay/RLC принят189279e. Следующий необходимый стык P6/P7 —
соединение электрических моделей с сохранением настоящих проводников и зарядов.
Это checkpoint проектирования; production-код — после review постановки.

## Результат задания

Предложить минимальное расширение существующего coupled-rlc Resource для
явного соответствия port -> positive/negative physical terminal. Показать один
TypeScript объект и native DTO, применение через существующий a.connect.
Не создавать новый DSL/универсальный connection framework.

Принципиальная граница: равенство двух дифференциальных напряжений и противоположные
port currents само по себе описывает идеальную связь портов, но не обязательно
электрическое соединение обоих физических контактов. Нельзя автоматически
объединять common modes, grounding infinity или выбрасывать внешнюю ёмкость.
Роли positive/negative должны быть однозначными и явными.

- Начальный класс: unreduced coupled модели с явно сохранёнными physical
  terminal potentials. Для general/noncanonical B и reduced basis описать,
  какая trace информация необходима; неоднозначное соответствие отвергать.
- Полная сборка должна сохранять R/L/C energy, Kirchhoff constraints, заряды
  и математические gauges. Различить zero-energy gauge и физический common charge.
- Предложить совместимость grounded/delay блоков и infinity моделей: где явно
  соединяется return, где нельзя соединять до дополнительных данных.
- P7 не должен дублировать time solver; external model sources не меняются.

## Обязательные независимые coupons в проекте

1. Две full-C модели с явным соединением двух контактов; сравнение с независимо
   собранной одной исходной схемой, включая nonzero common charge/energy.
2. Отличие соединения только одного контакта от обоих, отсутствие hidden ground.
3. Переименование/перестановка potential nodes и обращение port orientation.
4. Negatives: inconsistent B/terminal map, missing return, неизвестный reduced
   trace, неоднозначный initial charge и невозможная DC steady постановка.

Зафиксировать уравнения, знак currents, admission, сохранение domain/permissions,
конкретные numeric tolerances и оценку memory/time. Показать взаимодействие
с существующими coupled/composition и accepted mixed backend, не писать второй.
Записать план реализации/тестов в этот файл и краткую матрицу остатка P6.
Никаких builds/новых solves для документации. Не объявлять P6/P8/P11 закрытыми.
Не коммитить; сообщить готовый checkpoint и остановиться до review.

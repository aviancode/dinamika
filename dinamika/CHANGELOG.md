# Changelog

Все заметные изменения крейта `dinamika` документируются в этом файле.

Формат основан на [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
проект придерживается [семантического версионирования](https://semver.org/lang/ru/).

## [Unreleased]

## [0.1.0] - 2026-06-22

### Added

- Крейт-зонтик `dinamika`, объединяющий публичный API экосистемы.
- Реэкспорт растрового рендерера `dinamika-cpu` под модулем `cpu`.
- Реэкспорт библиотеки анимации `dinamika-core` под модулем `core`.
- Плоский реэкспорт API анимации (`pub use dinamika_core::*`) для удобного
  доступа без указания вложенного модуля.

[Unreleased]: https://github.com/aviancode/dinamika/compare/dinamika-v0.1.0...HEAD
[0.1.0]: https://github.com/aviancode/dinamika/releases/tag/dinamika-v0.1.0

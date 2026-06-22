# Commit stages — `dinamika`

Снимки файлов по стадиям для плана [`../commits.md`](../commits.md).

## Структура

```
commit-stages/
  <N>-<branch>/        одна папка на ветку
    <k>/               одна папка на коммит (1, 2, …)
      Cargo.toml       файлы в том виде, в каком они должны быть
      src/lib.rs       после этого коммита (с сохранением путей)
```

В каждой папке-коммите лежат **только файлы, которые этот коммит трогает**.

## Порядок применения

1. `1-develop-bootstrap/1` — библиотека-зонтик (`Cargo.toml` без `[[bin]]` +
   `src/lib.rs`).
2. `1-develop-bootstrap/2` — добавляет `[[bin]]`-цель в `Cargo.toml`.

```sh
cp -r commit-stages/1-develop-bootstrap/1/. .
git add -A && git commit -m "chore: scaffold dinamika umbrella crate"
cp -r commit-stages/1-develop-bootstrap/2/. .
git add -A && git commit -m "chore: add binary target for scenes"
```

Применив обе стадии, вы получите `Cargo.toml` и `src/lib.rs`, байт-в-байт
совпадающие с текущим состоянием крейта (проверено).

> ⚠️ `src/main.rs` для бинарной цели в репозитории нет — добавьте его сами
> (точка входа со сценами). См. примечание в [`../commits.md`](../commits.md).

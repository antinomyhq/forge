# Adapt Release Pipeline for Zetkolink/forgecode

## Objective

Переделать CI/CD pipeline и install-скрипт чтобы бинарники собирались и раздавались из нашего публичного форка `Zetkolink/forgecode`. Отдельный сервер **не нужен** — GitHub Releases + GitHub Pages (бесплатно).

## Architecture

```
User runs: curl -fsSL https://zetkolink.github.io/forgecode/install.sh | sh
                         │
                         ▼
            GitHub Pages (static file from repo)
                         │
                         ▼
    Downloads binary from GitHub Releases:
    https://github.com/Zetkolink/forgecode/releases/download/v1.0.0/forge-aarch64-apple-darwin
```

## Implementation Plan

### Phase 1: Simplify release.yml

- [ ] 1.1. Удалить job `npm_release` (строки 119-141) — не нужен, у нас нет npm-пакетов
- [ ] 1.2. Удалить job `homebrew_release` (строки 142-155) — не нужен
- [ ] 1.3. Удалить `POSTHOG_API_SECRET` из env в build steps (строки 109, 198, 293) — телеметрия не нужна
- [ ] 1.4. Оставить `build_release` job как есть — он уже использует только `GITHUB_TOKEN` (автоматический) и `APP_VERSION`

**Результат**: release.yml триггерится на `release: published`, собирает 9 бинарников и аплоадит их в GitHub Release. Никаких внешних секретов не нужно.

### Phase 2: Simplify ci.yml

- [ ] 2.1. Удалить `OPENROUTER_API_KEY` из env (строка 21) — не нужен для нашего форка
- [ ] 2.2. Удалить `POSTHOG_API_SECRET` из build_release и build_release_pr env (строки 198, 293)
- [ ] 2.3. Опционально: убрать `build_release_pr` job (строки 208-294) — PR builds с label `ci: build all targets`, полезно но необязательно

### Phase 3: Create install script

- [ ] 3.1. Создать `scripts/install.sh` — копия оригинального скрипта (780 строк) с заменой:
  - `antinomyhq/forge` → `Zetkolink/forgecode` (все URL загрузки бинарников)
  - `https://github.com/antinomyhq/forge#installation` → `https://github.com/Zetkolink/forgecode#installation`
  - Сохранить установку зависимостей (fzf, bat, fd) — URL-ы GitHub их репозиториев не меняются
- [ ] 3.2. Единственная строка которая формирует URL загрузки forge (строка 630-632 оригинала):
  ```
  DOWNLOAD_URLS="https://github.com/Zetkolink/forgecode/releases/latest/download/forge-$TARGET$TARGET_EXT"
  DOWNLOAD_URLS="https://github.com/Zetkolink/forgecode/releases/download/$VERSION/forge-$TARGET$TARGET_EXT"
  ```

### Phase 4: Host install script via GitHub Pages

- [ ] 4.1. Включить GitHub Pages в настройках репо: Settings → Pages → Source: `Deploy from a branch` → Branch: `main` → Folder: `/docs`
- [ ] 4.2. Создать `docs/install.sh` (или симлинк на `scripts/install.sh`) — GitHub Pages отдаст его как static file
- [ ] 4.3. Создать `docs/index.html` с редиректом на `install.sh` (опционально)
- [ ] 4.4. Альтернатива: использовать raw.githubusercontent.com напрямую:
  ```
  curl -fsSL https://raw.githubusercontent.com/Zetkolink/forgecode/main/scripts/install.sh | sh
  ```
  Тогда GitHub Pages не нужен вообще.

### Phase 5: First release

- [ ] 5.1. Запушить все изменения в main
- [ ] 5.2. Создать GitHub Release через UI или CLI: `gh release create v0.1.0 --title "v0.1.0" --notes "Initial release of fork"`
- [ ] 5.3. Дождаться CI — workflow `release.yml` соберёт 9 бинарников и прикрепит к релизу
- [ ] 5.4. Проверить установку: `curl -fsSL https://raw.githubusercontent.com/Zetkolink/forgecode/main/scripts/install.sh | sh`

## Secrets Required

| Secret | Нужен? | Откуда |
|--------|--------|--------|
| `GITHUB_TOKEN` | Да | Автоматический, не надо настраивать |
| `POSTHOG_API_SECRET` | Нет | Удаляем |
| `OPENROUTER_API_KEY` | Нет | Удаляем |
| `NPM_ACCESS` / `NPM_TOKEN` | Нет | Удаляем (вместе с npm job) |
| `HOMEBREW_ACCESS` | Нет | Удаляем (вместе с homebrew job) |

**Итого: ноль секретов для настройки.** Всё работает на автоматическом `GITHUB_TOKEN`.

## Install Command (финальный)

```bash
# Вариант 1: через raw.githubusercontent.com (без GitHub Pages)
curl -fsSL https://raw.githubusercontent.com/Zetkolink/forgecode/main/scripts/install.sh | sh

# Вариант 2: через GitHub Pages (если включить)
curl -fsSL https://zetkolink.github.io/forgecode/install.sh | sh
```

## Verification

- `release.yml` триггерится на publish release и собирает бинарники без ошибок
- Install-скрипт скачивает бинарник с `Zetkolink/forgecode` releases
- `forge --version` показывает версию после установки
- Зависимости (fzf, bat, fd) устанавливаются корректно

# System Monitor PWA для macOS

## 1. Цель

Разработать лёгкое локальное PWA-приложение для macOS, которое в реальном времени показывает:

* CPU;
* загрузку отдельных ядер;
* Performance / Efficiency cores;
* GPU;
* ANE, если доступно;
* оперативную память;
* memory pressure;
* compressed memory;
* swap;
* температуры;
* вентиляторы;
* энергопотребление;
* батарею;
* SSD;
* disk I/O;
* сеть;
* процессы;
* группы процессов;
* Docker-контейнеры;
* Node.js-процессы;
* IDE/dev-сервисы;
* историю всех основных показателей.

Никаких моков.

С первого запуска приложение должно работать на реальных данных текущего Mac.

Главная практическая задача:

> В любой момент пользователь должен иметь возможность открыть приложение и понять, что именно сейчас нагружает Mac, почему он греется, почему расходуется память и почему система может тормозить.

---

# 2. Целевая машина

Основная машина для разработки и первоначальной проверки:

```text
MacBook Pro
Mac15,3
Apple M3
CPU: 8 cores
4 Performance
4 Efficiency

Unified Memory:
8 GB
```

Архитектуру не привязывать жёстко к M3.

Должны корректно поддерживаться другие Apple Silicon Mac:

```text
M1
M2
M3
M4
M5
```

при наличии соответствующих датчиков.

---

# 3. Архитектура

Использовать следующую архитектуру:

```text
macOS
   │
   ├── Hardware Metrics
   │      CPU
   │      GPU
   │      ANE
   │      temperatures
   │      frequency
   │      fans
   │      power
   │
   ├── Native System Collector
   │      processes
   │      process tree
   │      RAM
   │      memory pressure
   │      disk
   │      network
   │      battery
   │      filesystem
   │
   ├── Docker Collector
   │      containers
   │      CPU
   │      RAM
   │      network
   │      block I/O
   │
   ▼
Local System Monitor Agent
   │
   ├── current state
   ├── aggregation
   ├── history
   ├── diagnostics
   ├── SQLite
   │
   ▼
WebSocket / HTTP API
   │
   ▼
React PWA
```

---

# 4. Никакого облака

Приложение полностью локальное.

Запрещено:

```text
cloud database
external analytics
telemetry
external backend
external account
registration
remote metrics collection
```

Все системные показатели остаются на Mac.

Backend должен слушать только:

```text
127.0.0.1
```

Никогда не:

```text
0.0.0.0
```

по умолчанию.

---

# 5. Backend

Backend/agent предпочтительно писать на:

```text
Rust
```

Причины:

* маленький memory footprint;
* низкий idle CPU;
* хороший доступ к системным API;
* один бинарник;
* удобный launchd;
* отсутствие Node.js backend, который сам становится объектом мониторинга.

Node.js НЕ использовать как backend System Monitor.

React/Vite используется только для frontend.

---

# 6. Hardware Metrics

Для метрик Apple Silicon использовать `macmon`.

Он должен давать:

```text
CPU usage
CPU active ratio
CPU frequency
P-core metrics
E-core metrics
per-core metrics

GPU usage
GPU frequency

ANE

CPU temperature
GPU temperature

fan RPM

CPU power
GPU power
ANE power

RAM
swap
```

Использовать sampling примерно:

```text
1000 ms
```

Не запускать новую команду `macmon` каждую секунду.

Должен существовать постоянно работающий источник данных.

Варианты реализации в порядке предпочтения:

### Вариант A

Интегрировать Rust library/API macmon непосредственно в System Monitor Agent.

### Вариант B

Запускать:

```bash
macmon serve --host 127.0.0.1
```

и получать `/json`.

Не делать shell spawn каждую секунду.

---

# 7. Процессы macOS

System Monitor Agent должен самостоятельно собирать список процессов.

Для каждого процесса нужно получать как минимум:

```text
PID
PPID
process name
executable
command line
user
CPU %
memory
physical footprint
threads
start time
state
disk read
disk write
```

Где системный API позволяет получить показатель без существенной нагрузки.

---

# 8. Частота процессов

Список процессов обновлять:

```text
1 раз в 1-2 секунды
```

Не нужно делать process enumeration 10 раз в секунду.

---

# 9. CPU процесса

CPU должен отображаться как реальная текущая нагрузка за sampling interval.

Например:

```text
node              74%
Docker             22%
WindowServer       16%
Safari             8%
```

Не показывать только накопленное CPU time.

Нужна именно текущая нагрузка.

---

# 10. Память процесса

Показывать:

```text
Resident memory
Physical footprint
```

Если доступно.

В основном интерфейсе приоритет отдать:

```text
Physical Footprint
```

поскольку он ближе к фактической нагрузке процесса на unified memory.

---

# 11. Command line

Обязательно получать командную строку процесса.

Плохой вариант:

```text
node
node
node
node
node
node
```

Хороший вариант:

```text
node
Vite dev server
~/Projects/unium/frontend

node
TypeScript Server
tsserver.js

node
Next.js
next dev

node
Codex worker
...
```

---

# 12. Node.js detection

Реализовать специальный classifier процессов Node.js.

Распознавать:

```text
node
npm
npx
yarn
pnpm
bun
vite
next
webpack
rollup
esbuild
tsserver
eslint
prettier
jest
vitest
playwright
electron
```

---

# 13. Определение Node-проекта

Если возможно определить:

```text
cwd
command arguments
parent process
package path
```

показывать проект.

Например:

```text
Node.js

Vite
unium-frontend

CPU      43%
RAM      612 MB

PID      18431
cwd      ~/Projects/unium/frontend
```

Это важнее, чем просто:

```text
node
```

---

# 14. Process Tree

Создавать дерево:

```text
Application
 └ process
    └ child process
       └ worker
```

Например:

```text
Visual Studio Code
 ├ Code Helper
 ├ TypeScript Server
 │   └ node
 ├ ESLint Server
 │   └ node
 └ Extension Host
```

---

# 15. Агрегация процессов

На верхнем уровне UI нужно показывать не только отдельные PID, но и агрегированные группы.

Например:

```text
VS Code
CPU       36%
RAM       2.1 GB

  Code
  Extension Host
  tsserver
  eslint
```

Или:

```text
Docker
CPU       48%
RAM       2.6 GB

  postgres
  redis
  backend
```

---

# 16. Aggregate metrics

Для process group считать сумму:

```text
CPU
RAM
disk read
disk write
```

по дочерним процессам.

Не суммировать показатели, где это математически некорректно.

---

# 17. Docker

Docker Desktop должен иметь специальную интеграцию.

Не ограничиваться отображением:

```text
com.docker.backend
```

---

# 18. Docker detection

Определять:

```text
Docker Desktop installed
Docker daemon available
Docker daemon unavailable
Docker not installed
```

Docker должен быть необязательной интеграцией.

Если Docker отсутствует, System Monitor продолжает нормально работать.

---

# 19. Docker Engine API

Подключаться напрямую к Docker Engine API через Unix socket / активный Docker context.

Не запускать:

```bash
docker stats
```

каждую секунду.

Использовать API.

Не хардкодить один путь Docker socket.

Учитывать:

```text
Docker Desktop
DOCKER_HOST
Docker context
```

---

# 20. Docker Containers

Для каждого контейнера показывать:

```text
name
container id
image
state

CPU %
memory used
memory limit

network receive
network transmit

block read
block write

PIDs
uptime
```

---

# 21. Docker UI

В списке процессов должна существовать группа:

```text
Docker
```

При раскрытии:

```text
Docker                         38%    2.4 GB

postgres                      12%    612 MB
unium-backend                 18%    921 MB
redis                          2%     86 MB
nginx                          1%     44 MB
```

---

# 22. Docker network

Показывать network I/O отдельно по контейнерам.

Например:

```text
unium-backend

↓ 4.2 MB/s
↑ 1.1 MB/s
```

---

# 23. Docker Disk I/O

Показывать:

```text
Read
Write
```

по контейнеру.

Это поможет находить контейнеры, интенсивно работающие с SSD.

---

# 24. Developer Processes

Создать специальную классификацию:

```text
Development
```

Распознавать минимум:

```text
Docker
Node.js
Vite
Next.js
Webpack
TypeScript
ESLint
VS Code
Cursor
Codex
Terminal
Git
Python
Java
PostgreSQL
Redis
```

---

# 25. Главный Process Monitor

Раздел:

```text
Processes
```

Колонки:

```text
Process
CPU
Memory
Disk
Network
Threads
Uptime
```

На небольшом экране часть колонок скрывается.

---

# 26. Быстрые фильтры

Сверху:

```text
All
Applications
Development
Docker
Node
System
```

---

# 27. Sort

Сортировка:

```text
CPU
Memory
Disk read
Disk write
Network
Name
```

По умолчанию:

```text
CPU descending
```

---

# 28. Search

Поиск должен находить:

```text
process name
command
path
project
PID
container
Docker image
```

Примеры:

```text
node

vite

unium

postgres

18431
```

---

# 29. Process Detail

При выборе процесса открывать drawer.

Показывать:

```text
process name

PID
PPID

CPU
RAM
physical footprint

threads

read
write

executable
command
cwd

started
uptime

parent

children
```

---

# 30. Process history

Для выбранного процесса показывать последние:

```text
CPU
RAM
disk I/O
```

за время его наблюдения System Monitor.

---

# 31. Не хранить бесконечную историю PID

PID переиспользуются системой.

Process identity строить как минимум из:

```text
PID
+
process start time
```

---

# 32. Overview Dashboard

Главный Dashboard должен сразу отвечать на вопросы:

```text
Насколько загружен Mac?

Хватает ли памяти?

Есть ли swap?

Греется ли Mac?

Что больше всего грузит CPU?

Кто использует память?

Кто пишет на SSD?

Есть ли активная Docker-нагрузка?

Есть ли тяжёлые Node процессы?
```

---

# 33. Верхние показатели

На Overview:

```text
CPU
GPU
Memory
Temperature
Power
Network
Battery
```

---

# 34. CPU Card

Показывать:

```text
current CPU

P cores
E cores

frequency

top process
```

Например:

```text
CPU
43%

P  61%
E  18%

3.2 GHz

Top:
node 24%
```

---

# 35. Memory Card

Особенно важный блок для 8 GB Mac.

Показывать:

```text
Used
Available
Compressed
Swap
Memory Pressure
```

Например:

```text
6.9 / 8 GB

Compressed
1.7 GB

Swap
2.4 GB

Pressure
Yellow
```

---

# 36. Memory Pressure

Использовать реальное состояние macOS memory pressure.

Не вычислять его исключительно:

```text
used / total
```

Память macOS нельзя оценивать только процентом Used RAM.

---

# 37. Температура

Показывать:

```text
CPU
GPU

fan RPM

thermal state
```

---

# 38. Power

Показывать:

```text
CPU W
GPU W
ANE W
Total SoC W
```

где показатели доступны.

---

# 39. Battery

Реальные:

```text
charge %
charging
power source
cycle count
health
time remaining
```

где macOS предоставляет данные.

---

# 40. Network

Показывать:

```text
current download
current upload
```

а также history.

Желательно определять активный interface:

```text
Wi-Fi
Ethernet
Thunderbolt
VPN
```

---

# 41. Disk

Показывать:

```text
free
used
total

read / sec
write / sec
```

Отдельно:

```text
current disk activity
```

---

# 42. SSD writes

Это важный показатель.

Показывать процессы с наибольшим:

```text
disk write
```

за последние:

```text
1 min
5 min
15 min
```

---

# 43. Main graph

Большой интерактивный realtime graph.

Tabs:

```text
CPU
GPU
Memory
Temperature
Power
Network
Disk
```

---

# 44. Периоды

Поддержать:

```text
1m
5m
15m
1h
6h
24h
7d
```

---

# 45. Live mode

Для:

```text
1m
5m
15m
```

график должен двигаться realtime.

Новые точки появляются без полной перерисовки страницы.

---

# 46. Tooltip

Hover показывает точные значения.

Например:

```text
14:32:18

CPU          48%
P cores      71%
E cores      26%
GPU          12%

CPU temp     67°C

Power        14.2 W
```

---

# 47. Realtime transport

Frontend не должен делать десятки HTTP polling requests.

Использовать:

```text
WebSocket
```

предпочтительно.

Допустим:

```text
Server-Sent Events
```

если WebSocket не требуется.

---

# 48. API

Например:

```text
GET /api/system
GET /api/processes
GET /api/process/:id
GET /api/history
GET /api/docker
GET /api/storage

WS /api/live
```

---

# 49. Live payload

Не отправлять каждую секунду огромный JSON со всей историей.

WebSocket передаёт только:

```text
current sample
changed processes
container changes
```

История запрашивается отдельно.

---

# 50. History

Нужна реальная история.

Не только данные с момента открытия PWA.

Backend работает постоянно и собирает данные независимо от того:

```text
открыто приложение
или
закрыто.
```

---

# 51. SQLite

Для хранения использовать:

```text
SQLite
```

Никаких Prometheus и Grafana.

Для одного Mac они избыточны.

---

# 52. Retention

Чтобы база оставалась маленькой, хранить разные resolution.

Например:

```text
последние 15 минут
1 sec

15-60 минут
5 sec

1-6 часов
15 sec

6-24 часа
1 min

1-7 дней
5 min

7-30 дней
15 min
```

Можно скорректировать после измерения реального размера БД.

---

# 53. Aggregation

При downsampling сохранять:

```text
average
minimum
maximum
```

для важных показателей.

Это позволит не потерять короткий temperature/CPU spike.

---

# 54. Processes history

Не нужно сохранять каждую секунду каждый процесс Mac.

Это быстро раздует БД.

Историю хранить:

### всегда

для top N процессов:

```text
CPU
Memory
Disk
```

### отдельно

для:

```text
Docker containers
Node/dev processes
```

### временно

для процесса, открытого пользователем.

---

# 55. Top Consumers

На Dashboard отдельный блок:

```text
Что сейчас грузит Mac
```

Пример:

```text
CPU

node · Vite · unium
32%

Docker · postgres
14%

WindowServer
9%
```

---

# 56. Memory Consumers

Отдельно:

```text
Memory

VS Code
2.1 GB

Docker
1.8 GB

Safari
1.2 GB

node · Vite
640 MB
```

---

# 57. Disk Consumers

Отдельно:

```text
Disk write

Docker · postgres
21 MB/s

node · Vite
8 MB/s

mds_stores
4 MB/s
```

---

# 58. Diagnostics

Блок:

```text
Почему Mac может тормозить?
```

Диагностика строится только на реальных данных.

---

# 59. Примеры диагностики

Например:

```text
Высокая memory pressure

За последние 10 минут Memory Pressure
была Yellow 72% времени.

Swap:
3.8 GB

Основные потребители:
VS Code 2.2 GB
Docker 1.9 GB
Safari 1.1 GB
```

---

Другой пример:

```text
Высокая CPU нагрузка

Средняя загрузка за 5 минут:
81%

Главные источники:

Vite
38%

Docker / postgres
21%

WindowServer
11%
```

---

Другой:

```text
Высокая запись на SSD

За последние 5 минут записано:
4.2 GB

Основной источник:
Docker / postgres
```

---

# 60. Не использовать LLM для диагностики

Диагностика должна быть локальной rule-based системой.

Никаких API.

---

# 61. Diagnostic Engine

Создать набор правил.

Например:

```text
memory pressure yellow > N sec

memory pressure red

swap growth

CPU > 85% > N sec

temperature > threshold

disk write sustained

network spike

single process CPU

single process memory
```

---

# 62. Не делать жёсткие выводы по одному sample

Например CPU 100% в течение одной секунды:

```text
нормально
```

CPU 95% в течение 10 минут:

```text
значимое событие
```

Диагностика должна учитывать duration.

---

# 63. Events

Хранить значимые события:

```text
15:42
Memory Pressure → Yellow

15:46
Swap > 3 GB

16:03
CPU > 90% for 5 min

16:14
Temperature peak 91°C
```

---

# 64. Design

Главный визуальный ориентир:

```text
premium macOS utility
```

а не:

```text
generic SaaS dashboard
```

---

# 65. Темы

Реализовать:

```text
Light
Dark
Auto
```

По умолчанию:

```text
Auto
```

через:

```text
prefers-color-scheme
```

---

# 66. Responsive

Приложение будет PWA и окно будет постоянно менять размер.

Обязательно качественно работать:

```text
430 × 600
600 × 700
768 × 700
900 × 700
1024 × 768
1280 × 800
1440 × 900
1728 × 1117
1920 × 1080
2560 × 1440
```

---

# 67. Layout

Использовать:

```text
CSS Grid
Container Queries
minmax()
clamp()
ResizeObserver
```

Не строить всё на JS `window.innerWidth`.

---

# 68. Sidebar

Desktop:

```text
Overview
Processes
Performance
Docker
Network
Disk
Sensors
History
```

Если Docker отсутствует:

раздел остаётся доступен, но показывает:

```text
Docker is not running
```

без ошибок.

---

# 69. Compact sidebar

При уменьшении окна:

```text
full sidebar
→
icon sidebar
→
drawer
```

---

# 70. Summary cards

На широком экране:

```text
CPU
GPU
Memory
Temperature
Network
Battery
```

При уменьшении:

```text
6
→
3
→
2
→
1/2
```

колонки.

Карточки сами адаптируют внутренний layout через Container Queries.

---

# 71. Process responsive UI

Desktop:

```text
Process | CPU | RAM | Disk | Network | Threads
```

Средний размер:

```text
Process | CPU | RAM | Disk
```

Узкий:

```text
Process | CPU | RAM
```

Очень узкий:

```text
Process | CPU
```

Клик раскрывает детали.

---

# 72. Sparklines

Каждый главный metric card имеет небольшую live history.

Например:

```text
CPU
43%

╭─╮  ╭────╮
  ╰──╯    ╰─
```

---

# 73. Motion

Допустимо:

```text
live number transitions
chart scrolling
hover
tooltip
status changes
theme transition
```

Не использовать декоративные spring animations.

---

# 74. Производительность самого System Monitor

Это критически важно.

Сам монитор не должен заметно нагружать Mac.

Цели:

```text
Agent idle CPU:
желательно < 1%

Frontend idle CPU:
минимально

Agent RAM:
десятки MB

Frontend:
по возможности < 100 MB
```

Измерить фактические значения.

---

# 75. Self Monitoring

System Monitor должен отображать самого себя в процессах.

Например:

```text
System Monitor Agent
CPU 0.4%
RAM 31 MB

System Monitor PWA
CPU 0.2%
RAM ...
```

Это позволит видеть его реальную стоимость.

---

# 76. Не обновлять весь React tree каждую секунду

Нельзя делать:

```text
setSystemState(wholeHugeObject)
```

на каждом sample и ререндерить весь Dashboard.

Разделить subscriptions.

---

# 77. Chart performance

График должен обновляться imperative API библиотеки.

Не пересоздавать chart каждую секунду.

Предпочтительно:

```text
uPlot
```

или аналогичный lightweight Canvas/SVG chart engine.

---

# 78. Visibility

Когда PWA находится в background:

Frontend может снизить частоту визуального обновления.

Backend продолжает сбор в обычном режиме.

После возврата:

графики догружают историю.

---

# 79. launchd

Agent должен автоматически запускаться после входа пользователя в macOS.

Использовать:

```text
launchd
```

---

# 80. Agent recovery

Если agent упал:

```text
launchd
```

должен его перезапустить.

---

# 81. PWA

Frontend:

```text
React
TypeScript
Vite
PWA
```

Manifest:

```text
display: standalone
```

После:

```text
Add to Dock
```

приложение должно ощущаться как desktop utility.

---

# 82. Local server

Agent должен раздавать frontend assets сам.

Например:

```text
http://127.0.0.1:9899
```

Таким образом нет отдельного Node/Vite production server.

---

# 83. Development mode

Vite используется только во время разработки.

Production:

```text
Rust agent
+
compiled static frontend
```

---

# 84. Local security

Несмотря на localhost:

проверять:

```text
Origin
Host
```

Не позволять произвольным внешним страницам управлять локальным API.

CORS по умолчанию закрытый.

---

# 85. Actions

На первом этапе System Monitor:

```text
READ ONLY
```

Не добавлять:

```text
Kill Process
Stop Container
Restart Docker
```

до отдельного этапа.

Сначала мониторинг должен быть безопасным.

---

# 86. Graceful degradation

Если определённая метрика недоступна:

не показывать:

```text
0
```

как будто это реальное значение.

Показывать:

```text
Unavailable
```

или скрывать показатель.

---

# 87. macmon unavailable

Если macmon не запущен:

System Monitor продолжает показывать:

```text
processes
RAM
network
disk
battery
```

а hardware sensors отмечаются:

```text
Sensor service unavailable
```

---

# 88. Auto reconnect

Frontend должен автоматически переподключаться к agent/WebSocket.

Например:

```text
Connected
Reconnecting
Disconnected
```

---

# 89. Первый запуск

При первом запуске:

1. Проверить backend.
2. Проверить macmon.
3. Проверить доступные sensors.
4. Проверить Docker.
5. Создать SQLite.
6. Запустить сбор.
7. Открыть Dashboard.

Не требовать ручной конфигурации.

---

# 90. Главная страница

Пример wide layout:

```text
┌──────────────────────────────────────────────────────┐
│ System Monitor        ● Live              Auto ⚙    │
├───────────┬──────────────────────────────────────────┤
│           │ CPU GPU MEMORY TEMP NETWORK BATTERY      │
│ Overview  │                                          │
│ Processes │ ┌───────────────────────┬──────────────┐ │
│ Docker    │ │                       │ MEMORY       │ │
│ Network   │ │ LIVE GRAPH            ├──────────────┤ │
│ Disk      │ │                       │ THERMAL      │ │
│ Sensors   │ └───────────────────────┴──────────────┘ │
│ History   │                                          │
│           │ TOP LOAD        PROCESSES     DIAGNOSIS │
└───────────┴──────────────────────────────────────────┘
```

---

# 91. Top Load

Очень важный компонент.

Показывает прямо сейчас:

```text
Highest CPU
node · Vite
31%

Highest memory
VS Code
2.1 GB

Highest disk write
Docker · postgres
18 MB/s

Highest network
Chrome
3.4 MB/s
```

---

# 92. Development widget

Если обнаружена developer activity, можно показывать:

```text
Development

Docker
CPU 34%
RAM 2.2 GB

Node.js
CPU 41%
RAM 1.3 GB

VS Code
RAM 1.8 GB
```

---

# 93. История рабочего дня

Раздел:

```text
History
```

позволяет посмотреть:

```text
CPU
RAM
Swap
Temperature
Power
Network
Disk
```

за день.

---

# 94. Очень полезный график

Отдельно построить:

```text
RAM + Compressed + Swap
```

на одном временном графике.

Для Mac с 8 GB это один из ключевых инструментов.

---

# 95. Memory event correlation

При hover исторического графика желательно показывать:

```text
в этот момент:

RAM
7.6 GB

Compressed
2.2 GB

Swap
4.1 GB

Top process
Docker 2.0 GB

Second
Code 1.9 GB
```

---

# 96. Quality requirement

Не считать первую версию законченной после того, как:

```text
данные появились
```

После реализации провести отдельный этап polish.

---

# 97. Проверка интерфейса

Сделать screenshots:

```text
1440 × 900 Light
1440 × 900 Dark
1024 × 768
800 × 800
600 × 800
430 × 700
```

Проверить:

```text
spacing
typography
density
responsive behavior
tables
charts
overflow
tooltips
sidebar
drawers
dark mode
```

Исправить найденные проблемы.

---

# 98. Проверка реальных сценариев

Провести тесты:

### Idle

Mac почти ничего не делает.

### CPU

Запустить CPU-heavy задачу.

System Monitor должен сразу показать процесс.

### Node

Запустить:

```text
npm run dev
```

Должно быть понятно:

```text
какой Node
какой проект
что именно он запускает
сколько CPU/RAM он использует
```

### Docker

Запустить несколько контейнеров.

System Monitor должен показать ресурсы каждого.

### Memory

Создать нагрузку на RAM.

Должны двигаться:

```text
RAM
compressed
swap
memory pressure
```

### Disk

Создать интенсивную запись.

Должен быть виден процесс/контейнер.

---

# 99. Acceptance criteria

Первая версия готова только если:

* данные настоящие;
* показатели обновляются автоматически;
* графики движутся realtime;
* история сохраняется при закрытом PWA;
* после перезагрузки Mac agent стартует сам;
* видны процессы;
* видно command line;
* Node процессы распознаются;
* видно проект Node, где возможно;
* Docker определяется;
* контейнеры показываются отдельно;
* CPU/RAM Docker-контейнеров реальные;
* отображается network и disk I/O контейнеров;
* видны top CPU consumers;
* видны top memory consumers;
* видны top disk consumers;
* memory pressure реальный;
* swap реальный;
* темы Light/Dark/Auto работают;
* responsive layout не ломается;
* UI качественно работает как PWA;
* сам монитор не создаёт заметной нагрузки.

---

# 100. Главный критерий готовности

Если компьютер начал тормозить, пользователь должен открыть System Monitor и за несколько секунд получить ответ примерно такого уровня:

```text
Основная причина:
высокая нагрузка Node.js.

unium/frontend
Vite dev server

CPU
68%

RAM
1.1 GB
```

или:

```text
Основная причина:
нехватка памяти.

Memory Pressure:
Yellow

Swap:
4.2 GB

Основные потребители:

VS Code
2.1 GB

Docker
1.9 GB

Safari
1.4 GB
```

или:

```text
Основная нагрузка Docker:

postgres
CPU 31%
RAM 740 MB
Disk write 22 MB/s

backend
CPU 18%
RAM 610 MB
```

Если интерфейс показывает только:

```text
CPU 84%
RAM 92%
```

но не позволяет быстро понять, **кто именно это вызвал**, задача выполнена недостаточно качественно.

Основная ценность приложения именно в сочетании:

```text
состояние системы
+
история
+
конкретные процессы
+
developer workloads
+
Docker containers
+
понятная диагностика
```

# Stash 随手

随手复制、随手存、随手取。Windows 上的剪贴板历史 + Snippets + 计算器启动器（Tauri 2 + Svelte 5）。

- `Alt+Space`（可在设置里改）唤起搜索框，统一搜索剪贴板历史和 snippets
- 模糊匹配 + 拼音（全拼 / 首字母），按匹配度和使用频率/时间排序
- 选中后自动切回原窗口并粘贴，可选粘贴后恢复原剪贴板
- Snippets 是普通 `.md` 文件，支持变量，目录可用任意网盘或 git 同步
- 直接输入算式即可计算，`$1 + $2` 引用最近复制的数字
- 网址类内容可用 `Ctrl ↵` 在浏览器打开

## 计算器

在搜索框里直接输入算式，结果会作为第一条出现，`↵` 粘贴结果，`⇧↵` 只复制。

| 写法 | 示例 |
|---|---|
| `+ - * / ^`、括号、`1e3`、`1_000` | `(1+2)*3^2` → 27 |
| `%` 取模、`!` 阶乘 | `10 % 3` → 1，`-7 % 3` → 2，`5!` → 120 |
| `$N` 剪贴板历史倒数第 N 条（1 = 最近） | 先复制 12 再复制 30，`$1+$2` → 42 |
| 函数 `sqrt cbrt abs sin cos tan asin acos atan ln log log2 exp floor ceil round` | `sqrt 2`、`log(1000)` |
| 常量 `pi π e tau`，角度 `deg` | `2pi`、`sin(30deg)` → 0.5 |
| 全角符号 | `（１＋２）×３÷２` → 4.5 |

只有包含运算符或函数时才算作计算，所以搜 `e`、`2024` 这类普通关键词不受影响。三角函数默认用弧度。

`$N` 引用的剪贴板内容会按数字读取，允许千分位逗号、空格、货币符号（`¥ $ € £`）、全角数字，以及结尾的 `%`（`15%` 读作 0.15）。如果引用的内容不是数字，就不显示计算结果。预览里会显示代入后的算式，方便核对取到的是哪个数。

## 快捷键

| 键 | 作用 |
|---|---|
| `↵` / `⇧↵` | 粘贴 / 仅复制 |
| `Ctrl ↵` | 用默认浏览器打开（内容是 http/https/www. 网址时） |
| `↑ ↓`、`Ctrl J/K` | 选择 |
| `Ctrl P` | 置顶 / 取消置顶剪贴板记录 |
| `Ctrl N` | 新建 snippet（选中剪贴板记录时以其内容为初稿） |
| `Ctrl E` | 编辑 snippet |
| `Ctrl D` | 删除（snippet 需按两次确认） |
| `Ctrl ,` | 设置 |
| `Esc` | 清空搜索 / 返回 / 隐藏 |

关闭后 60 秒内（可在设置里修改，0 表示不保留）重新打开，会保留上次的搜索内容并全选，直接输入即可替换；结果会重新计算，所以 `$1` 这类引用会用到最新的剪贴板。

## Snippet 格式

默认目录 `文档\Stash Snippets`，每个 snippet 一个文件：

```markdown
---
title: 周报
tags: [work]
---
本周（{{date:MM-dd}}）完成：
- {{cursor}}

下周计划：{{plan}}
```

| 变量 | 含义 |
|---|---|
| `{{name}}` `{{name=默认值}}` `{{env:prod\|dev}}` | 粘贴前弹窗填写（文本 / 带默认值 / 下拉） |
| `{{date}}` `{{time}}` `{{datetime}}` `{{date:yyyy年MM月dd日}}` | 当前时间，支持 `yyyy yy MM dd HH hh mm ss` |
| `{{clipboard}}` | 当前剪贴板文本 |
| `{{clipboard:N}}` | 剪贴板历史中倒数第 N 条（1 = 最近一条）；依次复制 A、B、C 后，`{{clipboard:3}}` 是 A |
| `{{uuid}}` | 随机 UUID |
| `{{cursor}}` | 粘贴后光标停留的位置 |

### 网址 snippet（快捷搜索）

内容是网址的 snippet 可以按 `Ctrl ↵` 在浏览器打开，变量照常先弹窗填写：

```markdown
---
title: 百度搜索
---
https://www.baidu.com/s?wd={{关键词}}
```

用浏览器打开时，网址里的变量值会自动做 URL 编码，中文、空格、`&` 都能正确传递；粘贴时则保持原样。如果整个网址本身就是一个变量（`{{url}}`），它的值不会被编码。只允许打开 `http`、`https` 和 `www.` 开头的地址。

使用次数等统计存在本地数据库而不是 snippet 文件里，所以使用 snippet 不会触发同步。

## 数据位置

- 配置：`%APPDATA%\io.github.liuchang1437.stash\config.json`
- 剪贴板历史：`%APPDATA%\io.github.liuchang1437.stash\stash.db`（SQLite，仅本地）

本项目早期名为 Box。首次以 Stash 启动时，会把 `%APPDATA%\com.box.app` 和 `文档\Box Snippets` 自动搬到上面的新位置（新位置已存在时不动）。

## 隐私

- 遵守 `ExcludeClipboardContentFromMonitorProcessing` / `CanIncludeInClipboardHistory` 标记（1Password、KeePassXC 等会设置）
- 可按程序名忽略，托盘菜单可暂停记录
- Stash 自己写入剪贴板的内容也带有排除标记，不会进入 Win+V 历史

## 开发

需要 Node.js、Rust（MSVC 工具链）和 VS Build Tools（C++）。

```bash
npm install
npm run tauri dev
```

```bash
npm run tauri build
```

Rust 单元测试：`cd src-tauri && cargo test`

## 结构

```
src-tauri/src/
  lib.rs          启动、托盘、热键、窗口、剪贴板监听、snippet 目录监听
  commands.rs     前端调用的命令（搜索、粘贴、置顶、编辑、设置）
  search.rs       内存索引：nucleo 模糊匹配 + 拼音 + frecency 排序
  calc.rs         计算器：表达式解析、$N 剪贴板引用
  template.rs     snippet 变量解析与渲染
  snippets.rs     .md 文件读写
  db.rs           SQLite：剪贴板历史、snippet 使用统计
  config.rs       设置
  migrate.rs      从旧名 Box 迁移数据
  platform/       系统相关实现（目前只有 Windows）
src/lib/          Svelte 界面：Launcher / FillForm / SnippetEditor / SettingsView
```

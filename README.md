# tx_launch

## Introduction

tx_launch is an command line tool for launching android apps.
it uses cmd line tools like 'am'(activity manager) and 'pm'(package manager) for launching apps.

---

## Prerequisites

### 1. Termux
- **Source:** GitHub or F-Droid builds only

### 2. AAPT
Command line tool for android asset packaging.

**Installation:**
```bash
apt install aapt -y
```

### 3. Action build of termux *(Optional but Recommended)*
Download from either:
- [tx_launch Releases](https://github.com/BayonetArch/tx_launch/releases)
- [Official Termux-App GitHub Repository](https://github.com/termux/termux-app)

---

## Installation

### Option 1: Building from Source

**Requirements:**
- rust installed in termux

**Steps:**

1. Install Rust:
```bash
apt install rust -y
```

2. Setup storage access:
```bash
termux-setup-storage
```
then restart Termux.

3. Clone the repository:
```bash
git clone https://github.com/BayonetArch/tx_launch.git
```

4. Build the project:
```bash
cargo build --release
```

The resulting binary will be located at: `target/release/tx_launch`

### Option 2: Download Pre-built Binary

Download directly from the [Releases section](https://github.com/BayonetArch/tx_launch/releases)

---

## Usage

`tx_launch --help` for help.

### activity manager options

By default, the tool uses termux's built-in `am` (`/data/data/com.termux/files/usr/bin/am`), which runs on JVM and is very slow.

#### Available AM Options:

| Option | Description | Requirements |
|--------|-------------|--------------|
| `old` | default termux am (slowest) | None |
| `new` | termux-am from gitHub action builds (faster) | gitHub action build of yermux + "Display over other apps" permission |
| `system` | system am (fastest) | android 10 only |

**Change am:**
```bash
tx_launch --am {new,old,system}
```

### Launching Apps

By default, launching the tool opens a REPL where you can type app names:
```bash
tx_launch
```

#### Direct Launch
Launch apps directly from the command line:
```bash
tx_launch --run {app_name}
```

### Example

```bash
tx_launch --run playstore --am new
```
*Launches play store using the new am*

---

## Notes

- when using `new` am, ensure termux has "Display over other apps" permission enabled
- system am only works on android 10

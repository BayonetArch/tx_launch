# Introduction: #

`tx_launch` is an command line tool for launching android apps.
it uses cmd line tools like 'am'(activity manager) and 'pm'(package manager) for launching apps.

# Prerequisites #
- termux(github,f-droid builds only)
- aapt(cmd line tool)
    install it in termux :
    ```bash 
    apt install aapt -y 
    ```
- action build of termux(optional but recommended).
    you can download it in release section of this repo:https://github.com/BayonetArch/tx_launch/releases 

    or download from official termux-app github repo

# installation #
1.Building from source<br>
  to build the tool from source make sure you have rust installed.

```bash 
 apt install rust -y 
 ```
 also make sure you do 

 ```bash 
 termux-setup-storage 
 ```
 and restart termux.clone the repo:
 ```bash
git clone https://github.com/BayonetArch/tx_launch.git
```
 
 
 
 then you can just 

```bash 
 Cargo build --release
```
the resultant binary wiil be in `target/release/tx_launch`

 2. Download the binary<br>
 if you do not want to build it and you can download it from the releases section
           https://github.com/BayonetArch/tx_launch/releases

# Usage #
type `tx_launch --help` to see options.

By default the tool will use termux `builtin am`(/data/data/com.termux/files/usr/bin/am) which uses jvm hence is very slow.
if you want to use`new` am(termux-am) which is faster than the native you need to use the new apk builds of termux(github action builds) and also make sure you provide 'Display over other apps' permission to termux otherwise it won't work.
and lastly `system` am will only work on android 10.it is the fastest.

To change the `am` use :

```bash
tx_launch --am {new,old,system}

```
By default launching the tool will take you to repl where you can type app names to launch aps.if you want to launch app directly:

```bash
tx_launch --run {app_name}

```
# Example #

```bash
tx_launch --run playstore --am new # launch playstore using new am

```


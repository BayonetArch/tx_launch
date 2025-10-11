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

 2. Download the binary
 if you do not want to build it and you can download it from the releases section
           https://github.com/BayonetArch/tx_launch/releases

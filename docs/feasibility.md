# 可行性报告
- [可行性报告](#可行性报告)
  - [理论依据](#理论依据)
    - [FreeRTOS 架构](#freertos-架构)
    - [Rust 语言特性](#rust-语言特性)
      - [所有权](#所有权)
      - [Unsafe](#unsafe)
      - [FFI(Foreign Function Interface)](#ffiforeign-function-interface)
      - [条件编译](#条件编译)
    - [Multicore](#multicore)
  - [技术依据](#技术依据)
    - [调试与运行环境](#调试与运行环境)
      - [编译](#编译)
      - [使用qemu仿真](#使用qemu仿真)
      - [开发板环境仿真](#开发板环境仿真)
    - [Capability Mechanism](#capability-mechanism)
  - [项目设计](#项目设计)
  - [创新点](#创新点)
  - [参考文献](#参考文献)
## 理论依据
### FreeRTOS 架构
TODO
### Rust 语言特性
#### 所有权
TODO
#### Unsafe
TODO
#### FFI(Foreign Function Interface)
TODO
#### 条件编译
TODO
### Multicore
TODO
## 技术依据
### 调试与运行环境
环境采用目前工业主流的stm32板块
#### 编译
+ 安装对应三元组配置
  ```bash
  rustup target add thumbv7m-none-eabi
  ```
+ 编译生成ELF文件
  ```bash
  cargo build --target=thumbv7m-none-eabi
  ```
+ binutils 工具集
为了查看和分析生成的可执行文件，我们首先需要安装一套名为 binutils 的命令行工具集，其中包含了 objdump、objcopy 等常用工具。

Rust 社区提供了一个 cargo-binutils 项目，可以帮助我们方便地调用 Rust 内置的 LLVM binutils。我们用以下命令安装它

```bash
cargo install cargo-binutils

rustup component add llvm-tools-preview
```
+ 通过 objcopy 转换为二进制文件
  生成的文件为ELF格式， 为能够加载到内存中实现，需要利用objcopy转换
```bash
objcopy -O binary os os.bin
```

#### 使用qemu仿真
此处参考了GitHub上一个项目[**FreeRTOS-GCC-ARM926ejs**](https://github.com/jkovacic/FreeRTOS-GCC-ARM926ejs)
使用
```bash
qemu-system-arm -M versatilepb -nographic -m 128 -kernel image.bin
```
即可进行仿真测试
#### 开发板环境仿真
此处则是需要利用`STM32Cube`生成芯片上的环境
STM32Cube可以生成所选芯片的MakeFile，在本地`make`后，利用工具链`arm-none-gcc`
```bash
arm-none-eabi-objcopy -O binary -S build/for_stm32f429.elf build/for_stm32f429.bin
``` 
之后使用`qemu-system-gnuarmeclipse`可以运行出结果

### Capability Mechanism
TODO
## 项目设计
TODO
## 创新点
TODO
## 参考文献

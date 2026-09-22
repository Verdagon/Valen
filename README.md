
# Valen 

Valen is a programming language that's aims to be not only **fast** and **memory-safe**, but also **easy and flexible**.

NOTE: Valen is _still a prototype_ and barely past the proof-of-concept stage. There are holes and sharp edges. We'll release a 0.1 version once it's stable enough to use, stay tuned!

Our plans for Valen:

 * **Speed:** Valen is AOT compiled to LLVM, statically-typed, and aims to be the fastest native language, by giving more fine-grained aliasing information to LLVM. 
 * **Safety:** For memory safety and data-race safety, it is the uses the new [group borrowing](https://verdagon.dev/blog/group-borrowing) technique, which is like a more flexible borrow checking, with mutable aliasing.
 * **Flexibility:** We'll be adding generational references and reference counting, which should be usable without `Cell`, `RefCell`, etc.

## Running a Valen Program

 1. Make a directory for your Valen project:
    * `mkdir my_valen_project`
    * `cd my_valen_project`
    * `mkdir src`
    * Make a `src/main.valen` containing `exported func main() int { return 42; }`
 2. Build the Valen compiler:
    * Clone the repo, `git clone https://github.com/valen-lang/valen`
    * `cd valen`
    * `cargo build --bin valec`
 3. Compile and run your Valen project:
    * Compile: `./target/debug/valec build --no-std --builtins-dir-override src/builtins/resources main=test.vale`
    * Run: `build/main`
    * See the result: `echo $?` (should be `42`)

## Historical Notes

Valen is the successor to the [Vale programming language](https://vale.dev/).

Thank you to everyone who sponsored Vale! Vale existed because of your support, and Valen exists because Vale existed. Thank you to all of Vale's sponsors:

 * [Arthur Weagel](https://github.com/aweagel)
 * [Kiril Mihaylov](https://github.com/KirilMihaylov)
 * [Radek Miček](https://github.com/radekm)
 * [Geomitron](https://github.com/Geomitron)
 * [Chiuzon](https://github.com/chiuzon)
 * [Felix Scholz](https://github.com/soupertonic)
 * [Joseph Jaoudi](https://github.com/linkmonitor)
 * [Luke Puchner-Hardman](https://github.com/lupuchard)
 * [Jonathan Zielinski](https://github.com/tootoobeepbeep)
 * [Albin Kocheril Chacko](https://github.com/albinkc)
 * [Enrico Zschemisch](https://github.com/ezschemi)
 * [Svintooo](https://github.com/Svintooo)
 * [Tim Stack](https://github.com/tstack)
 * [Alon Zakai](https://github.com/kripken)
 * [Alec Newman](https://github.com/rovaughn)
 * [Sergey Davidoff](https://github.com/Shnatsel)
 * [Ian (linuxy)](https://github.com/linuxy)
 * [Ivo Balbaert](https://github.com/Ivo-Balbaert/)
 * [Pierre Curto](https://github.com/pierrec)
 * [Love Jesus](https://github.com/loveJesus)
 * [J. Ryan Stinnett](https://github.com/jryans)
 * [Cristian Dinu](https://github.com/cdinu)
 * [Florian Plattner](https://github.com/lasernoises)

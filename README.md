# 連続体

An exploration on a faster CI for personal use.

It doesn't attempt to solve all problems, just the ones I have and
mainaining a good balance between speed and simplicity.

## Main Ideas

* Pre-generate the pipeline environment with the necessary repos and packages.
* Use ephemeral [ZONES(7)](https://system-illumination.org/man/zones.7.html) as isolation mechanism.
* Use ZFS to very quickly clone the environment when a pipeline is triggered.
* Use a simple configuration format with a CLI-based editor similar to how [ZONECFG(8)](https://system-illumination.org/man/zonecfg.8.html) works.

## Example

```sh
    $ cicfg -p katarineko
    cicfg:katarineko>create
    cicfg:katarineko> add package
    cicfg:katarineko:package> set name="elixir"
    cicfg:katarineko:package> set provider="pkgsrc"
    cicfg:katarineko:package> end
    cicfg:katarineko> add package
    cicfg:katarineko:package> set name="rust"
    cicfg:katarineko:package> set provider="pkgsrc"
    cicfg:katarineko:package> end
    cicfg:katarineko> info package
    elixir from pkgsrc
    rust from pkgsrc
    cicfg:katarineko> add repo
    cicfg:katarineko:repo> set url="https://github.com/MarceColl/katarineko"
    cicfg:katarineko:repo> end
    cicfg:katarineko> add step
    cicfg:katarineko:step> set name="build"
    cicfg:katarineko:step> set script="build.sh"
    cicfg:katarineko:step> end
    cicfg:katarineko> add step
    cicfg:katarineko:step> set name="test"
    cicfg:katarineko:step> set script="test.sh"
    cicfg:katarineko:step> set depends="build"
    cicfg:katarineko:step> end
    cicfg:katarineko> commit

    Creating pipeline katarineko
    Created ZFS dataset at rpool/zones/teisuu/katarineko/base
    Installing zone...DONE
    Booting zone...DONE
    Installing packages (elixir rust)...DONE
    Executing step build...DONE
    Executing step test...DONE

    Pipeline katarineko created.

    Pipeline URL: <string>https://ci.gyoju.net/pipeline/katarineko</string>
     Webhook URL: <string>https://ci.gyoju.net/pipeline/katarineko/webhook</string>

```

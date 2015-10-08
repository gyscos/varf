# varf

varf is an Arff file viewer.

It reads a file given as input, then starts a small web server to allow the user to browse the data.

It should compile properly even on rust stable:

```
git clone https://github.com/Gyscos/varf
cd varf
cargo build
```


```
Usage: varf [OPTIONS] FILENAME

Options:
    -h --help           Prints this help message.
    -p PORT             Sets the port to listen to.
    -d VARF_HOME        Sets the directory where varf files are installed.
                        Defaults to /usr/share/varf
    -o, --open          Open the page in the browser
```


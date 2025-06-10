#kill other instances
pkill -f wire-storm

#Build and run
cargo build --release
./target/release/wire-storm &

#Run tests
python3 python_tests/tests.py

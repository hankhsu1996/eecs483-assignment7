for i in {1..1000}
do
    cargo test | tee result.txt
    ret=$?
    if [ $ret -ne 0 ]; then
        exit
    fi
    grep FAILED result.txt
    ret=$?
    if [ $ret -eq 0 ]; then
        exit
    fi
done

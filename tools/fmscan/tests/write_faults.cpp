// Fault injection is confined to this test executable, never a production flag.
#include "../write_transaction.hpp"
int main(int argc,char** argv){
    if(argc!=3)return 2;
    std::string mode=argv[1];auto p=load_plan(argv[2]);int calls=0;
    WriteFunction fault=[&](pid_t pid,U addr,const std::vector<uint8_t>& bytes)->ssize_t{
        ++calls;
        if(mode=="fail-second"&&calls==2)return -1;
        if(mode=="rollback-fail"&&calls>=2)return -1;
        if(mode=="partial-first"&&calls==1){std::vector<uint8_t> prefix{bytes[0]};return native_write(pid,addr,prefix);}
        auto n=native_write(pid,addr,bytes);
        if(mode=="crash-first"&&calls==1)_exit(86);
        if(mode=="change-first"&&calls==1){std::vector<uint8_t> third{17};native_write(pid,addr,third);}
        return n;
    };
    try{transact(argv[2],false,true,fault);return 0;}
    catch(const std::exception& e){std::cerr<<e.what()<<"\n";return 1;}
}

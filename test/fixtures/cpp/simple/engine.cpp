#include <vector>
#include "util.h"

namespace mylib {

class Engine {
public:
    Engine(int size);
    void run();
private:
    int count_;
};

Engine::Engine(int size) : count_(0) {}

void Engine::run() {
    std::vector<int> v;
    v.push_back(helper(1, 2));
}

} // namespace mylib

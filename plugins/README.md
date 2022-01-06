# Miner Plugins

**CAUTION! The plugin api is brand new and might change without prior notice.** 

**CAUTION! Old plugins might not be compatible with new plugins: check the plugin version.** 

**CAUTION! Plugins can run arbitrary code: if you use precompiled, make sure they come from 
reputable source**

The plugin system relies on three interfaces defined in `lib.rs` on `kaspa-miner`. 
Each interface refers to an object which has a different job:
  * **Plugin** - the environment and configuration of a type of workers.
  * **WorkerSpec** - Light weight struct containing the initialization arguments for a worker.
  Can be (and is) sent between threads.
  * **Worker** - The worker object, which contains references to device memory and functions. Usually not thread safe.

To implemenet your own plugin, create a `crate`, and implement the required methods. Build the as a `cdylib`
and place it in the plugins directory. Add the plugin names to `main.rs` code to whitelist it.
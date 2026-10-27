    // Contract Management
    function adjustWage(delta) {
      const el = document.getElementById("contract-wage");
      adjustMoneyInput("contract-wage", delta, 1000);
      autoSaveContract("wage_per_week");
    }

    function autoSaveContract(fieldName) {
      if (fieldName) {
        state.playerDirtyFields.add("contract." + fieldName);
      } else {
        state.playerDirtyFields.add("contract");
      }
      markUnsaved();
    }

    async function saveContractToGame() {
      return applyPlayerChanges();
    }


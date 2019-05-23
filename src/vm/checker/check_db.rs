use rusqlite::{Connection, Savepoint, OptionalExtension, NO_PARAMS, Row};
use rusqlite::types::ToSql;


use vm::types::TypeSignature;
use vm::checker::errors::{CheckError, CheckErrors, CheckResult};
use vm::checker::typecheck::{ContractAnalysis, FunctionType};

const SQL_FAIL_MESSAGE: &str = "PANIC: SQL Failure in contract analysis.";

pub struct AnalysisDatabase <'a> {
    savepoint: Savepoint<'a>
}

pub struct AnalysisDatabaseConnection {
    conn: Connection
}

impl AnalysisDatabaseConnection {
    pub fn initialize(filename: &str) -> AnalysisDatabaseConnection {
        let contract_db = AnalysisDatabaseConnection::inner_open(filename);
        // this is the _laziest_ of structures at the moment.
        //    more to come!
        contract_db.conn.execute("CREATE TABLE IF NOT EXISTS type_analysis_table
                      (contract_identifier INTEGER PRIMARY KEY AUTOINCREMENT,
                       contract_name TEXT NOT NULL UNIQUE,
                       analysis TEXT NOT NULL)",
                            NO_PARAMS)
            .expect(SQL_FAIL_MESSAGE);
        
        contract_db.check_schema();

        contract_db
    }

    pub fn memory() -> AnalysisDatabaseConnection {
        AnalysisDatabaseConnection::initialize(":memory:")
    }

    pub fn open(filename: &str) -> AnalysisDatabaseConnection {
        let contract_db = AnalysisDatabaseConnection::inner_open(filename);

        contract_db.check_schema();
        contract_db
    }

    pub fn check_schema(&self) {
        let sql = "SELECT sql FROM sqlite_master WHERE name=?";
        let _: String = self.conn.query_row(sql, &["type_analysis_table"],
                                            |row| row.get(0))
            .expect("Bad schema in analysis db initialization.");
    }

    pub fn inner_open(filename: &str) -> AnalysisDatabaseConnection {
        let conn = Connection::open(filename)
            .expect("Failure to open analysis db.");
        AnalysisDatabaseConnection {
            conn: conn
        }
    }

    pub fn begin_save_point(&mut self) -> AnalysisDatabase<'_> {
        AnalysisDatabase::from_savepoint(self.conn.savepoint()
                                         .expect(SQL_FAIL_MESSAGE))
    }
}

impl <'a> AnalysisDatabase <'a> {

    pub fn from_savepoint(sp: Savepoint<'a>) -> AnalysisDatabase<'a> {
        AnalysisDatabase {
            savepoint: sp }
    }

    pub fn roll_back(&mut self) {
        self.savepoint.rollback()
            .expect(SQL_FAIL_MESSAGE);
    }

    pub fn commit(self) {
        self.savepoint.commit()
            .expect(SQL_FAIL_MESSAGE);
    }

    pub fn execute<P>(&mut self, sql: &str, params: P) -> usize
    where
        P: IntoIterator,
        P::Item: ToSql {
        self.savepoint.execute(sql, params)
            .expect(SQL_FAIL_MESSAGE)
    }

    fn query_row<T, P, F>(&self, sql: &str, params: P, f: F) -> Option<T>
    where
        P: IntoIterator,
        P::Item: ToSql,
        F: FnOnce(&Row) -> T {
        self.savepoint.query_row(sql, params, f)
            .optional()
            .expect(SQL_FAIL_MESSAGE)
    }

    fn load_contract(&self, contract_name: &str) -> Option<ContractAnalysis> {
        let result: Option<String> = self.query_row(
            "SELECT analysis FROM type_analysis_table WHERE contract_name = ?",
            &[contract_name],
            |row| row.get(0));
        match result {
            Some(contract) => Some(ContractAnalysis::deserialize(&contract)),
            None => None
        }
    }

    pub fn get_public_function_type(&self, contract_name: &str, function_name: &str) -> CheckResult<FunctionType> {
        let contract = self.load_contract(contract_name)
            .ok_or(CheckError::new(CheckErrors::NoSuchContract(contract_name.to_string())))?;
        Ok(
            contract.get_public_function_type(function_name)
                .ok_or(CheckError::new(CheckErrors::NoSuchPublicFunction(contract_name.to_string(),
                                                                         function_name.to_string())))?
                .clone())
    }

    pub fn get_map_type(&self, contract_name: &str, map_name: &str) -> CheckResult<(TypeSignature, TypeSignature)> {
        let contract = self.load_contract(contract_name)
            .ok_or(CheckError::new(CheckErrors::NoSuchContract(contract_name.to_string())))?;
        let map_type = contract.get_map_type(map_name)
            .ok_or(CheckError::new(CheckErrors::NoSuchMap(map_name.to_string())))?;
        Ok(map_type.clone())
    }

    pub fn insert_contract(&mut self, contract_name: &str, contract: &ContractAnalysis) -> CheckResult<()> {
        if self.load_contract(contract_name).is_some() {
            return Err(CheckError::new(CheckErrors::ContractAlreadyExists(contract_name.to_string())))
        }
        self.execute(
            "INSERT INTO type_analysis_table (contract_name, analysis) VALUES (?, ?)",
            &[contract_name, &contract.serialize()]);
        Ok(())
    }
}
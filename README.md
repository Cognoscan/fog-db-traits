Interfaces for fog-db
=====================

There's more than one way to implement a database for fog-db, and this crate is 
dedicated to creating a common API for dealing with fog-db databases. It 
defines:

- The cursor API for simultaneous, streaming navigation through both local and 
	remote databases.
- The transaction API for modifying the local database
- The certificate and policy API for setting access policies into the database
- The Group API for opening up a group of connections to remote database nodes

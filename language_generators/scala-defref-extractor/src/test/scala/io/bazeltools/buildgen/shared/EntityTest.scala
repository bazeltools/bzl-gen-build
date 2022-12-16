package io.bazeltools.buildgen.shared

import org.scalacheck.Gen

class EntityTests extends munit.ScalaCheckSuite {
  val genEntity: Gen[Entity] =
    for {
      cnt <- Gen.choose(1, 5)
      idents <- Gen.listOfN(cnt, Gen.identifier)
    } yield Entity(idents.toVector)

  property("Entity Ordering is lawful") {
    OrderingLaws.orderingLaws(genEntity)
  }
}

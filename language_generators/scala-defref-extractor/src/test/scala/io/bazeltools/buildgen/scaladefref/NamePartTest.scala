package io.bazeltools.buildgen.scaladefref

import org.scalacheck.Gen
import org.scalacheck.Prop.forAll
import io.bazeltools.buildgen.shared.Entity
import cats.data.NonEmptyList

class NamePartTests extends munit.ScalaCheckSuite {

  val genNonPackage: Gen[NamePart] =
    Gen.oneOf(
      Gen.const(NamePart.Anonymous),
      Gen.identifier.map { nm => NamePart.Defn(Entity.simple(nm)) }
    )

  val genPackage: Gen[NamePart.Package] =
    Gen.choose(1, 5).flatMap(Gen.listOfN(_, Gen.identifier)).map { es =>
      NamePart.Package(Entity(es.toVector))
    }

  val genNamePart: Gen[NamePart] =
    Gen.oneOf(genNonPackage, genPackage)

  val genParts: Gen[Vector[NamePart]] = Gen.listOf(genNamePart).map(_.toVector)

  test("zero packages means top level") {
    assertEquals(
      NamePart.referencePackages(Vector.empty),
      NonEmptyList(Entity.empty, Nil)
    )
  }

  property("nested packages add linearly to the scopes") {
    forAll(Gen.listOf(genPackage), Gen.listOf(genPackage)) { (p1, p2) =>
      val left = NamePart.referencePackages(p1)
      val right = NamePart.referencePackages(p1 ::: p2)
      val rightOnly = right.toList.drop(left.length)

      // left is a strict prefix
      assertEquals(left.toList, right.toList.take(left.length))
      val lastLeft = left.last
      rightOnly.foreach { path =>
        assert(path.startsWith(lastLeft))
      }
    }
  }

  property("entities are in increasing length in referencePackages") {
    forAll(genParts) { parts =>
      val rps = NamePart.referencePackages(parts)
      rps.toList.sliding(2).foreach {
        case Seq(a, b) =>
          assert(a.parts.length < b.parts.length)
        case other =>
          assertEquals(other.length, 1)
      }
    }
  }

  property("root is always first referencePackages") {
    forAll(genParts) { parts =>
      val rps = NamePart.referencePackages(parts)
      assertEquals(rps.head, Entity.empty)
    }
  }

  property("anonymous or defn cuts off packages") {
    forAll(genParts, genNonPackage, genParts) { (first, nonPack, last) =>
      assertEquals(
        NamePart.referencePackages(first :+ nonPack :++ last),
        NamePart.referencePackages(first)
      )
    }
  }
}
